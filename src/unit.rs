use bytes::{Bytes, Buf};
use i2cdev::core::I2CDevice;
use i2cdev::linux::LinuxI2CDevice;

use crate::error::*;
use crate::metric::Metric;

fn read_two_byte_unsigned_float(uint_part: u8, frac_part: u8) -> f32 {
  uint_part as f32 + (frac_part as f32 / 10f32)
}

pub trait MetrifulUnit: Sized + Default {
  type Output;

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output>;

  fn new_metric(register: u8) -> Metric<Self> {
    Metric {
      register,
      unit: Self::default()
    }
  }
}

#[derive(Default, Debug)]
pub struct UnitDegreesCelsius;

impl MetrifulUnit for UnitDegreesCelsius {
  type Output = f32;

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, 2)?);
    let int_part = bytes.get_i8();
    let frac_part = bytes.get_u8();

    Ok(int_part as f32 + (frac_part as f32 / 10f32))
  }
}

#[derive(Default, Debug)]
pub struct UnitPascals;

impl MetrifulUnit for UnitPascals {
  type Output = u32; // signed?

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, 4)?);
    Ok(bytes.get_u32_le())
  }
}

#[derive(Default, Debug)]
pub struct UnitAWeightedDecibels;

impl MetrifulUnit for UnitAWeightedDecibels {
  type Output = f32;

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, 2)?);
    let uint_part = bytes.get_u8();
    let frac_part = bytes.get_u8();

    Ok(read_two_byte_unsigned_float(uint_part, frac_part))
  }
}

pub struct DecibelBands {

}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitDecibelBands;

impl MetrifulUnit for UnitDecibelBands {
  type Output = DecibelBands;

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, 12)?);

    // TODO
    Ok(DecibelBands {})
  }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum SoundMeasurementStability {
  /// Microphone initialization has finished
  Stable,

  /// Microphone initialization still ongoing
  Unstable
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitSoundMeasurementStability;

impl MetrifulUnit for UnitSoundMeasurementStability {
  type Output = SoundMeasurementStability;

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    match device.smbus_read_byte_data(register)? {
      1 => Ok(SoundMeasurementStability::Stable),
      _ => Ok(SoundMeasurementStability::Unstable),
    }
  }
}

