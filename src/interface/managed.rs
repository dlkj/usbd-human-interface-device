use core::cell::RefCell;
use core::marker::PhantomData;

use delegate::delegate;
use embedded_time::duration::Milliseconds;
use log::error;
use packed_struct::PackedStruct;
use usb_device::bus::UsbBus;
use usb_device::class_prelude::*;
use usb_device::UsbError;

use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::InterfaceNumber;
use crate::interface::{HidProtocol, UsbAllocatable};
use crate::interface::{InterfaceClass, WrappedInterface};
use crate::UsbHidError;

pub struct IdleManager<R> {
    last_report: Option<R>,
    current: Milliseconds<u32>,
    default: Milliseconds<u32>,
    since_last_report: Milliseconds<u32>,
}

impl<R> IdleManager<R>
where
    R: Eq + Copy,
{
    pub fn new(default: Milliseconds<u32>) -> Self {
        Self {
            last_report: None,
            current: default,
            default,
            since_last_report: Milliseconds(0),
        }
    }

    pub fn reset(&mut self) {
        self.last_report = None;
        self.current = self.default;
        self.since_last_report = Milliseconds(0);
    }

    pub fn report_written(&mut self, report: R) {
        self.last_report = Some(report);
        self.since_last_report = Milliseconds(0);
    }

    pub fn set_duration(&mut self, duration: Milliseconds) {
        self.current = duration;
    }

    pub fn is_duplicate(&self, report: &R) -> bool {
        self.last_report.as_ref() == Some(report)
    }

    pub fn tick(&mut self) -> bool {
        if self.current == Milliseconds(0u32) {
            self.since_last_report = Milliseconds(0);
            return false;
        }

        if self.since_last_report >= self.current {
            self.since_last_report = Milliseconds(0);
            true
        } else {
            self.since_last_report = self.since_last_report + Milliseconds(1u32);
            false
        }
    }

    pub fn last_report(&self) -> Option<R> {
        self.last_report
    }
}

pub struct ManagedInterface<'a, B: UsbBus, R> {
    inner: RawInterface<'a, B>,
    idle_manager: RefCell<IdleManager<R>>,
}

impl<'a, B: UsbBus, R, const LEN: usize> ManagedInterface<'a, B, R>
where
    R: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
{
    pub fn write_report(&self, report: &R) -> Result<(), UsbHidError> {
        if self.idle_manager.borrow().is_duplicate(report) {
            self.tick()?;
            Err(UsbHidError::Duplicate)
        } else {
            let data = report.pack().map_err(|e| {
                error!("Error packing BootKeyboardReport: {:?}", e);
                UsbHidError::SerializationError
            })?;

            self.inner
                .write_report(&data)
                .map_err(UsbHidError::from)
                .map(|_| {
                    self.idle_manager.borrow_mut().report_written(*report);
                })
        }
    }

    /// Call every 1ms / at 1 KHz
    pub fn tick(&self) -> Result<(), UsbHidError> {
        let mut idle_manager = self.idle_manager.borrow_mut();
        if !(idle_manager.tick()) {
            Ok(())
        } else if let Some(r) = idle_manager.last_report() {
            let data = r.pack().map_err(|e| {
                error!("Error packing BootKeyboardReport: {:?}", e);
                UsbHidError::SerializationError
            })?;
            match self.inner.write_report(&data) {
                Ok(n) => {
                    idle_manager.report_written(r);
                    Ok(n)
                }
                Err(UsbError::WouldBlock) => Err(UsbHidError::WouldBlock),
                Err(e) => Err(UsbHidError::UsbError(e)),
            }
            .map(|_| ())
        } else {
            Ok(())
        }
    }

    delegate! {
        to self.inner{
            pub fn read_report(&self, data: &mut [u8]) -> usb_device::Result<usize>;
        }
    }
}

impl<'a, B: UsbBus, R> InterfaceClass<'a> for ManagedInterface<'a, B, R>
where
    R: Copy + Eq,
{
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'_ [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'_ str>;
           fn set_report(&mut self, data: &[u8]) -> usb_device::Result<()>;
           fn get_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize>;
           fn get_report_ack(&mut self) -> usb_device::Result<()>;
           fn get_idle(&self, report_id: u8) -> u8;
           fn set_protocol(&mut self, protocol: HidProtocol);
           fn get_protocol(&self) -> HidProtocol;
        }
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.idle_manager.borrow_mut().reset();
    }
    fn set_idle(&mut self, report_id: u8, value: u8) {
        self.inner.set_idle(report_id, value);
        if report_id == 0 {
            self.idle_manager
                .borrow_mut()
                .set_duration(self.inner.global_idle());
        }
    }
}

impl<'a, B: UsbBus, R> WrappedInterface<'a, B, RawInterface<'a, B>, ()>
    for ManagedInterface<'a, B, R>
where
    R: Copy + Eq,
{
    fn new(interface: RawInterface<'a, B>, _config: ()) -> Self {
        let default_idle = interface.global_idle();
        Self {
            inner: interface,
            idle_manager: RefCell::new(IdleManager::new(default_idle)),
        }
    }
}

pub struct ManagedInterfaceConfig<'a, R> {
    report: PhantomData<R>,
    inner_config: RawInterfaceConfig<'a>,
}

impl<'a, R> ManagedInterfaceConfig<'a, R> {
    pub fn new(inner_config: RawInterfaceConfig<'a>) -> Self {
        Self {
            inner_config,
            report: Default::default(),
        }
    }
}

impl<'a, B, R> UsbAllocatable<'a, B> for ManagedInterfaceConfig<'a, R>
where
    B: UsbBus + 'a,
    R: Copy + Eq,
{
    type Allocated = ManagedInterface<'a, B, R>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        ManagedInterface::new(self.inner_config.allocate(usb_alloc), ())
    }
}
