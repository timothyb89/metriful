#[macro_use] extern crate log;

use sysfs_gpio::Pin;
use i2cdev::core::*;
use i2cdev::linux::{LinuxI2CDevice};


pub mod unit;
pub mod metric;
pub mod error;

use error::*;
use unit::*;
use metric::*;

/// Metriful i2c address. Note: 0x70 if solder bridge is closed.
pub const METRIFUL_ADDRESS: u16 = 0x71;

pub struct Metriful {
  ready_pin: Pin,
  device: LinuxI2CDevice
}

impl Metriful {
  pub fn new(ready_pin: Pin, device: LinuxI2CDevice) -> Metriful {
    Metriful {
      ready_pin, device
    }
  }

  pub fn is_ready(&self) -> Result<bool> {
    Ok(self.ready_pin.get_value()? == 0)
  }
}

impl Metriful {
  pub fn reset(&mut self) -> Result<()> {
    self.device.smbus_write_byte(0xE2)?;

    Ok(())
  }

  pub fn read<U: MetrifulUnit>(&mut self, metric: &Metric<U>) -> Result<U::Output> {
    metric.read(&mut self.device)
  }
}
