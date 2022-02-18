use core::cell::RefCell;
use core::marker::PhantomData;

use delegate::delegate;
use embedded_time::duration::Milliseconds;
use embedded_time::timer::param::{OneShot, Running};
use embedded_time::{Clock, Timer};
use log::error;
use packed_struct::PackedStruct;
use usb_device::bus::UsbBus;
use usb_device::class_prelude::*;
use usb_device::UsbError;

use crate::interface::raw::{RawInterface, RawInterfaceConfig, WrappedRawInterface};
use crate::interface::InterfaceClass;
use crate::interface::InterfaceNumber;
use crate::interface::{HidProtocol, UsbAllocatable};
use crate::UsbHidError;

pub struct IdleManager<'a, R, C: Clock> {
    clock: &'a C,
    last_report: Option<R>,
    current: Milliseconds,
    default: Milliseconds,
    timer: Option<Timer<'a, OneShot, Running, C, Milliseconds>>,
}

impl<'a, R: Eq + Copy, C: Clock<T = u64>> IdleManager<'a, R, C> {
    pub fn new(clock: &'a C, default: Milliseconds) -> Self {
        Self {
            clock,
            last_report: None,
            current: default,
            default,
            timer: None,
        }
    }

    pub fn reset(&mut self) {
        self.last_report = None;
        self.timer = None;
        self.current = self.default;
    }

    pub fn report_written(&mut self, report: R) {
        self.last_report = Some(report);
        self.reset_timer();
    }

    pub fn reset_timer(&mut self) {
        if self.current == Milliseconds(0u32) {
            self.timer = None
        } else {
            self.timer = Some(self.clock.new_timer(self.current).start().unwrap())
        }
    }

    pub fn set_duration(&mut self, duration: Milliseconds) {
        self.current = duration;

        if let Some(t) = self.timer.as_ref() {
            let elapsed: Milliseconds = t.elapsed().unwrap();
            if elapsed > self.current {
                //sending a report is now overdue, set a zero time timer
                self.timer = Some(self.clock.new_timer(Milliseconds(0)).start().unwrap())
            } else {
                //carry over elapsed time
                self.timer = Some(
                    self.clock
                        .new_timer(self.current - elapsed)
                        .start()
                        .unwrap(),
                )
            }
        } else {
            self.reset_timer();
        }
    }

    pub fn is_duplicate(&self, report: &R) -> bool {
        self.last_report.as_ref() == Some(report)
    }

    pub fn is_idle_expired(&self) -> bool {
        self.timer
            .as_ref()
            .map(|t| t.is_expired().unwrap())
            .unwrap_or_default()
    }

    pub fn last_report(&self) -> Option<&R> {
        self.last_report.as_ref()
    }
}

pub trait WrappedManagedInterface<'a, B: UsbBus, C: Clock<T = u64>, Report, Config = ()>:
    Sized + InterfaceClass<'a>
{
    fn new(interface: ManagedInterface<'a, B, C, Report>, config: Config) -> Self;
    fn tick(&self) -> Result<(), UsbHidError>;
}

pub struct ManagedInterface<'a, B: UsbBus, C: Clock<T = u64>, R> {
    inner: RawInterface<'a, B>,
    idle_manager: RefCell<IdleManager<'a, R, C>>,
}

impl<'a, B: UsbBus, C: Clock<T = u64>, R, const LEN: usize> ManagedInterface<'a, B, C, R>
where
    R: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
{
    pub fn write_report(&self, report: &R) -> Result<usize, UsbHidError> {
        if self.idle_manager.borrow().is_duplicate(report) {
            self.tick()?;
            Err(UsbHidError::Duplicate)
        } else {
            let data = report.pack().map_err(|e| {
                error!("Error packing BootKeyboardReport: {:?}", e);
                UsbHidError::SerializationError
            })?;
            match self.inner.write_report(&data) {
                Ok(n) => {
                    self.idle_manager.borrow_mut().report_written(*report);
                    Ok(n)
                }
                Err(UsbError::WouldBlock) => Err(UsbHidError::WouldBlock),
                Err(e) => Err(UsbHidError::UsbError(e)),
            }
        }
    }

    pub fn tick(&self) -> Result<(), UsbHidError> {
        let mut idle_manager = self.idle_manager.borrow_mut();
        if !(idle_manager.is_idle_expired()) {
            Ok(())
        } else if let Some(r) = idle_manager.last_report() {
            let data = r.pack().map_err(|e| {
                error!("Error packing BootKeyboardReport: {:?}", e);
                UsbHidError::SerializationError
            })?;
            match self.inner.write_report(&data) {
                Ok(n) => {
                    idle_manager.reset_timer();
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

impl<'a, B: UsbBus, C: Clock<T = u64>, R> InterfaceClass<'a> for ManagedInterface<'a, B, C, R>
where
    R: Copy + Eq,
{
    delegate! {
        to self.inner{
           fn report_descriptor(&self) -> &'a [u8];
           fn id(&self) -> InterfaceNumber;
           fn write_descriptors(&self, writer: &mut DescriptorWriter) -> usb_device::Result<()>;
           fn get_string(&self, index: StringIndex, _lang_id: u16) -> Option<&'static str>;
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

impl<'a, B: UsbBus, C: Clock<T = u64>, R> WrappedRawInterface<'a, B, &'a C>
    for ManagedInterface<'a, B, C, R>
where
    R: Copy + Eq,
{
    fn new(interface: RawInterface<'a, B>, config: &'a C) -> Self {
        let default_idle = interface.global_idle();
        Self {
            inner: interface,
            idle_manager: RefCell::new(IdleManager::new(config, default_idle)),
        }
    }
}

pub struct WrappedManagedInterfaceConfig<'a, I, R, CLOCK, C = ()>
where
    CLOCK: Clock<T = u64>,
    R: Copy + Eq,
{
    inner_config: RawInterfaceConfig<'a>,
    clock: &'a CLOCK,
    config: C,
    interface: PhantomData<I>,
    report: PhantomData<R>,
}

impl<'a, I, R, CLOCK, C> WrappedManagedInterfaceConfig<'a, I, R, CLOCK, C>
where
    CLOCK: Clock<T = u64>,
    R: Copy + Eq,
{
    pub fn new(inner_config: RawInterfaceConfig<'a>, clock: &'a CLOCK, config: C) -> Self {
        Self {
            inner_config,
            clock,
            config,
            interface: Default::default(),
            report: Default::default(),
        }
    }
}

impl<'a, I, R, B, CLOCK, C> UsbAllocatable<'a, B>
    for WrappedManagedInterfaceConfig<'a, I, R, CLOCK, C>
where
    I: WrappedManagedInterface<'a, B, CLOCK, R, C>,
    B: UsbBus + 'a,
    CLOCK: Clock<T = u64>,
    R: Copy + Eq,
{
    type Allocated = I;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        I::new(
            ManagedInterface::new(self.inner_config.allocate(usb_alloc), self.clock),
            self.config,
        )
    }
}
