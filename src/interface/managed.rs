use core::marker::PhantomData;

use fugit::{ExtU32, MillisDurationU32};
use packed_struct::PackedStruct;
use usb_device::bus::UsbBus;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::UsbError;

use crate::interface::raw::{Interface, InterfaceConfig};
use crate::interface::{DeviceClass, UsbAllocatable};
use crate::UsbHidError;

use super::raw::{InSize, OutSize, ReportSingle};

struct IdleManager<R> {
    last_report: Option<R>,
    since_last_report: MillisDurationU32,
}

impl<R> Default for IdleManager<R> {
    fn default() -> Self {
        Self {
            last_report: Option::None,
            since_last_report: 0.millis(),
        }
    }
}

impl<R> IdleManager<R>
where
    R: Eq + Copy,
{
    pub fn report_written(&mut self, report: R) {
        self.last_report = Some(report);
        self.since_last_report = 0.millis();
    }

    pub fn is_duplicate(&self, report: &R) -> bool {
        self.last_report.as_ref() == Some(report)
    }

    /// Call every 1ms
    pub fn tick(&mut self, timeout: MillisDurationU32) -> bool {
        if timeout.ticks() == 0 {
            self.since_last_report = 0.millis();
            return false;
        }

        if self.since_last_report >= timeout {
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

pub struct ManagedIdleInterface<'a, B: UsbBus, Report, I, O>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    inner: Interface<'a, B, I, O, ReportSingle>,
    idle_manager: IdleManager<Report>,
}

#[allow(clippy::inline_always)]
impl<'a, B: UsbBus, Report, I, O> ManagedIdleInterface<'a, B, Report, I, O>
where
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    fn new(interface: Interface<'a, B, I, O, ReportSingle>, _config: ()) -> Self {
        Self {
            inner: interface,
            idle_manager: IdleManager::default(),
        }
    }
}

#[allow(clippy::inline_always)]
impl<'a, B: UsbBus, Report, I, O, const LEN: usize> ManagedIdleInterface<'a, B, Report, I, O>
where
    Report: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    pub fn write_report(&mut self, report: &Report) -> Result<(), UsbHidError> {
        if self.idle_manager.is_duplicate(report) {
            Err(UsbHidError::Duplicate)
        } else {
            let data = report.pack().map_err(|_| {
                error!("Error packing report");
                UsbHidError::SerializationError
            })?;

            self.inner
                .write_report(&data)
                .map_err(UsbHidError::from)
                .map(|_| {
                    self.idle_manager.report_written(*report);
                })
        }
    }

    pub fn read_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize> {
        self.inner.read_report(data)
    }
}

impl<'a, B: UsbBus, Report, I, O, const LEN: usize> DeviceClass<'a>
    for ManagedIdleInterface<'a, B, Report, I, O>
where
    Report: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
    B: UsbBus,
    I: InSize,
    O: OutSize,
{
    type I = Interface<'a, B, I, O, ReportSingle>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.inner
    }

    fn reset(&mut self) {
        self.idle_manager = IdleManager::default();
    }

    fn tick(&mut self) -> Result<(), UsbHidError> {
        if !(self.idle_manager.tick(self.inner.global_idle())) {
            Ok(())
        } else if let Some(r) = self.idle_manager.last_report() {
            let data = r.pack().map_err(|_| {
                error!("Error packing report");
                UsbHidError::SerializationError
            })?;
            match self.inner.write_report(&data) {
                Ok(n) => {
                    self.idle_manager.report_written(r);
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
}

pub struct ManagedIdleInterfaceConfig<'a, Report, I, O>
where
    I: InSize,
    O: OutSize,
{
    report: PhantomData<Report>,
    inner_config: InterfaceConfig<'a, I, O, ReportSingle>,
}

impl<'a, Report, I, O> ManagedIdleInterfaceConfig<'a, Report, I, O>
where
    I: InSize,
    O: OutSize,
{
    #[must_use]
    pub fn new(inner_config: InterfaceConfig<'a, I, O, ReportSingle>) -> Self {
        Self {
            inner_config,
            report: PhantomData::default(),
        }
    }
}

impl<'a, B, Report, I, O> UsbAllocatable<'a, B> for ManagedIdleInterfaceConfig<'a, Report, I, O>
where
    B: UsbBus + 'a,
    I: InSize,
    O: OutSize,
{
    type Allocated = ManagedIdleInterface<'a, B, Report, I, O>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        ManagedIdleInterface::new(self.inner_config.allocate(usb_alloc), ())
    }
}
