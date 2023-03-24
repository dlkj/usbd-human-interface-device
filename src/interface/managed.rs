use core::marker::PhantomData;

use fugit::{ExtU32, MillisDurationU32};
use packed_struct::PackedStruct;
use usb_device::bus::UsbBus;
#[allow(clippy::wildcard_imports)]
use usb_device::class_prelude::*;
use usb_device::UsbError;

use crate::interface::raw::{RawInterface, RawInterfaceConfig};
use crate::interface::InterfaceClass;
use crate::interface::UsbAllocatable;
use crate::UsbHidError;

pub struct IdleManager<R> {
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

pub struct ManagedInterface<'a, B: UsbBus, R, const IN_BUF: usize, const OUT_BUF: usize> {
    inner: RawInterface<'a, B, IN_BUF, OUT_BUF>,
    idle_manager: IdleManager<R>,
}

#[allow(clippy::inline_always)]
impl<'a, B: UsbBus, R, const IN_BUF: usize, const OUT_BUF: usize>
    ManagedInterface<'a, B, R, IN_BUF, OUT_BUF>
where
    R: Copy + Eq,
{
    fn new(interface: RawInterface<'a, B, IN_BUF, OUT_BUF>, _config: ()) -> Self {
        Self {
            inner: interface,
            idle_manager: IdleManager::default(),
        }
    }
}

#[allow(clippy::inline_always)]
impl<'a, B: UsbBus, R, const IN_BUF: usize, const OUT_BUF: usize, const LEN: usize>
    ManagedInterface<'a, B, R, IN_BUF, OUT_BUF>
where
    R: Copy + Eq + PackedStruct<ByteArray = [u8; LEN]>,
{
    pub fn write_report(&mut self, report: &R) -> Result<(), UsbHidError> {
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

    /// Call every 1ms
    pub fn tick(&mut self) -> Result<(), UsbHidError> {
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

    pub fn read_report(&mut self, data: &mut [u8]) -> usb_device::Result<usize> {
        self.inner.read_report(data)
    }
}

impl<'a, B: UsbBus, R, const IN_BUF: usize, const OUT_BUF: usize> InterfaceClass<'a, B>
    for ManagedInterface<'a, B, R, IN_BUF, OUT_BUF>
where
    R: Copy + Eq,
{
    type I = RawInterface<'a, B, IN_BUF, OUT_BUF>;

    fn interface(&mut self) -> &mut Self::I {
        &mut self.inner
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.idle_manager = IdleManager::default();
    }
}

pub struct ManagedInterfaceConfig<'a, R, const IN_BUF: usize, const OUT_BUF: usize> {
    report: PhantomData<R>,
    inner_config: RawInterfaceConfig<'a, IN_BUF, OUT_BUF>,
}

impl<'a, R, const IN_BUF: usize, const OUT_BUF: usize>
    ManagedInterfaceConfig<'a, R, IN_BUF, OUT_BUF>
{
    #[must_use]
    pub fn new(inner_config: RawInterfaceConfig<'a, IN_BUF, OUT_BUF>) -> Self {
        Self {
            inner_config,
            report: PhantomData::default(),
        }
    }
}

impl<'a, B, R, const IN_BUF: usize, const OUT_BUF: usize> UsbAllocatable<'a, B>
    for ManagedInterfaceConfig<'a, R, IN_BUF, OUT_BUF>
where
    B: UsbBus + 'a,
    R: Copy + Eq,
{
    type Allocated = ManagedInterface<'a, B, R, IN_BUF, OUT_BUF>;

    fn allocate(self, usb_alloc: &'a UsbBusAllocator<B>) -> Self::Allocated {
        ManagedInterface::new(self.inner_config.allocate(usb_alloc), ())
    }
}
