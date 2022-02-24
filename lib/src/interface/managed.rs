use core::cell::RefCell;
use core::marker::PhantomData;

use delegate::delegate;
use embedded_time::duration::Milliseconds;
use embedded_time::timer::param::{OneShot, Running};
use embedded_time::{TimeInt, Timer};
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

pub struct IdleManager<'a, R, C: embedded_time::Clock> {
    clock: &'a C,
    last_report: Option<R>,
    current: Milliseconds,
    default: Milliseconds,
    timer: Option<Timer<'a, OneShot, Running, C, Milliseconds>>,
}

impl<'a, R, C, Tick> IdleManager<'a, R, C>
where
    R: Eq + Copy,
    C: embedded_time::Clock<T = Tick>,
    Tick: TimeInt,
    u32: TryFrom<Tick>,
{
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
            let elapsed = t.elapsed().unwrap();
            if elapsed > self.current {
                //sending a report is now overdue, set a zero time timer
                self.timer = Some(self.clock.new_timer(Milliseconds(0)).start().unwrap())
            } else {
                //carry over elapsed time
                let c = self.current;
                let milliseconds = c - elapsed;
                let remaining = milliseconds;
                self.timer = Some(
                    self.clock
                        .new_timer::<Milliseconds>(remaining)
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

pub struct ManagedInterface<'a, B: UsbBus, Clock: embedded_time::Clock, R> {
    inner: RawInterface<'a, B>,
    idle_manager: RefCell<IdleManager<'a, R, Clock>>,
}

impl<'a, B: UsbBus, Clock: embedded_time::Clock, Tick, R, const LEN: usize>
    ManagedInterface<'a, B, Clock, R>
where
    R: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
    Clock: embedded_time::Clock<T = Tick>,
    Tick: TimeInt,
    u32: TryFrom<Tick>,
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

impl<'a, B: UsbBus, C: embedded_time::Clock, Tick, R> InterfaceClass<'a>
    for ManagedInterface<'a, B, C, R>
where
    R: Copy + Eq,
    C: embedded_time::Clock<T = Tick>,
    Tick: TimeInt,
    u32: TryFrom<Tick>,
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

impl<'a, B: UsbBus, C: embedded_time::Clock, Tick, R>
    WrappedInterface<'a, B, RawInterface<'a, B>, &'a C> for ManagedInterface<'a, B, C, R>
where
    R: Copy + Eq,
    C: embedded_time::Clock<T = Tick>,
    Tick: TimeInt,
    u32: TryFrom<Tick>,
{
    fn new(interface: RawInterface<'a, B>, config: &'a C) -> Self {
        let default_idle = interface.global_idle();
        Self {
            inner: interface,
            idle_manager: RefCell::new(IdleManager::new(config, default_idle)),
        }
    }
}

pub struct ManagedInterfaceConfig<'a, Clock, R> {
    clock: &'a Clock,
    report: PhantomData<R>,
    inner_config: RawInterfaceConfig<'a>,
}

impl<'a, Clock, R> ManagedInterfaceConfig<'a, Clock, R> {
    pub fn new(inner_config: RawInterfaceConfig<'a>, clock: &'a Clock) -> Self {
        Self {
            inner_config,
            clock,
            report: Default::default(),
        }
    }
}

impl<'a, B, Clock, Tick, R> UsbAllocatable<'a, B> for ManagedInterfaceConfig<'a, Clock, R>
where
    B: UsbBus + 'a,

    Clock: embedded_time::clock::Clock<T = Tick>,
    Tick: TimeInt,
    u32: TryFrom<Tick>,
    R: Copy + Eq,
{
    type Allocated = ManagedInterface<'a, B, Clock, R>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        ManagedInterface::new(self.inner_config.allocate(usb_alloc), self.clock)
    }
}
