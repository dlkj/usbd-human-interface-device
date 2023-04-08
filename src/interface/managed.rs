use core::cell::RefCell;
use core::marker::PhantomData;

use fugit::{ExtU32, MillisDurationU32};
use log::error;
use packed_struct::PackedStruct;
use usb_device::bus::UsbBus;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::UsbError;

use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::UsbAllocatable;
use crate::interface::{InterfaceClass, WrappedInterface};
use crate::UsbHidError;

pub struct IdleManager<R> {
    last_report: Option<R>,
    current_timeout: MillisDurationU32,
    default_timeout: MillisDurationU32,
    since_last_report: MillisDurationU32,
}

impl<R> IdleManager<R>
where
    R: Eq + Copy,
{
    #[must_use]
    pub fn new(default: MillisDurationU32) -> Self {
        Self {
            last_report: None,
            current_timeout: default,
            default_timeout: default,
            since_last_report: 0.millis(),
        }
    }

    pub fn reset(&mut self) {
        self.last_report = None;
        self.current_timeout = self.default_timeout;
        self.since_last_report = 0.millis();
    }

    pub fn report_written(&mut self, report: R) {
        self.last_report = Some(report);
        self.since_last_report = 0.millis();
    }

    pub fn set_duration(&mut self, duration: MillisDurationU32) {
        self.current_timeout = duration;
    }

    pub fn is_duplicate(&self, report: &R) -> bool {
        self.last_report.as_ref() == Some(report)
    }

    /// Call every 1ms
    pub fn tick(&mut self) -> bool {
        if self.current_timeout.ticks() == 0 {
            self.since_last_report = 0.millis();
            return false;
        }

        if self.since_last_report >= self.current_timeout {
            self.since_last_report = 0.millis();
            true
        } else {
            self.since_last_report += 1.millis();
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

#[allow(clippy::inline_always)]
impl<'a, B: UsbBus, R, const LEN: usize> ManagedInterface<'a, B, R>
where
    R: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
{
    pub fn write_report(&self, report: &R) -> Result<(), UsbHidError> {
        if self.idle_manager.borrow().is_duplicate(report) {
            Err(UsbHidError::Duplicate)
        } else {
            let data = report.pack().map_err(|e| {
                error!("Error packing report: {:?}", e);
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

    /// Call every 1ms
    pub fn tick(&self) -> Result<(), UsbHidError> {
        let mut idle_manager = self.idle_manager.borrow_mut();

        idle_manager.set_duration(self.inner.global_idle());

        if !(idle_manager.tick()) {
            Ok(())
        } else if let Some(r) = idle_manager.last_report() {
            let data = r.pack().map_err(|e| {
                error!("Error packing report: {:?}", e);
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

    pub fn read_report(&self, data: &mut [u8]) -> usb_device::Result<usize> {
        self.inner.read_report(data)
    }
}

impl<'a, B: UsbBus, R> InterfaceClass<'a, B> for ManagedInterface<'a, B, R>
where
    R: Copy + Eq,
{
    #![allow(clippy::inline_always)]

    fn interface(&self) -> &RawInterface<'a, B> {
        &self.inner
    }

    fn interface_mut(&mut self) -> &mut RawInterface<'a, B> {
        &mut self.inner
    }

    fn reset(&mut self) {
        self.idle_manager.borrow_mut().reset();
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
    #[must_use]
    pub fn new(inner_config: RawInterfaceConfig<'a>) -> Self {
        Self {
            inner_config,
            report: PhantomData::default(),
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
