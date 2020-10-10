use i2cdev::linux::LinuxI2CDevice;
use lazy_static::lazy_static;

use crate::error::*;
use crate::unit::*;

pub struct Metric<U> where U: MetrifulUnit {
  pub register: u8,
  pub unit: U
}

impl<U> Metric<U> where U: MetrifulUnit {
  pub fn read(&self, d: &mut LinuxI2CDevice) -> Result<U::Output> {
    U::read(d, self.register)
  }
}

fn metric<U>(register: u8) -> Metric<U>
where
  U: MetrifulUnit
{
  U::new_metric(register)
}

// TODO: make these const when const generics lands
lazy_static! {
  pub static ref METRIC_TEMPERATURE: Metric<UnitDegreesCelsius> = metric(0x21);
  pub static ref METRIC_PRESSURE: Metric<UnitPascals> = metric(0x22);
}
