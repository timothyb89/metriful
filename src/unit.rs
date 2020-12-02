use std::fmt;

use bytes::{Bytes, Buf};
use i2cdev::core::I2CDevice;
use i2cdev::linux::LinuxI2CDevice;

use crate::error::*;
use crate::metric::Metric;
use crate::util::*;

#[derive(Debug)]
struct UnitSymbol(Option<&'static str>);

impl fmt::Display for UnitSymbol {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    if let Some(symbol) = self.0 {
      write!(f, "{}", symbol)
    } else {
      write!(f, "none")
    }
  }
}

impl From<&'static str> for UnitSymbol {
  fn from(o: &'static str) -> Self {
    UnitSymbol(Some(o))
  }
}

impl From<Option<&'static str>> for UnitSymbol {
  fn from(o: Option<&'static str>) -> Self {
    UnitSymbol(o)
  }
}

pub trait MetrifulUnit: Sized + Default + fmt::Debug {
  /// This unit's native datatype.
  type Output: fmt::Display;

  /// The human-readable name of the unit
  fn name() -> &'static str;

  /// The human-readable symbol for this unit
  fn symbol() -> Option<&'static str>;

  fn format_value(value: Self::Output) -> String {
    if let Some(symbol) = Self::symbol() {
      format!("{} {}", value, symbol)
    } else {
      format!("{}", value)
    }
  }

  /// Reads the appropriate value for this unit from the given register.
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

  fn name() -> &'static str {
    "degrees Celsius"
  }

  fn symbol() -> Option<&'static str> {
    "\u{2103}C".into()
  }

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
  type Output = u32;

  fn name() -> &'static str {
    "pascals"
  }

  fn symbol() -> Option<&'static str> {
    Some("Pa")
  }

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, 4)?);
    Ok(bytes.get_u32_le())
  }
}

#[derive(Default, Debug)]
pub struct UnitAWeightedDecibels;

impl MetrifulUnit for UnitAWeightedDecibels {
  type Output = f32;

  fn name() -> &'static str {
    "A-weighted decibels"
  }

  fn symbol() -> Option<&'static str> {
    Some("dBa")
  }

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, 2)?);
    let uint_part = bytes.get_u8();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(uint_part, frac_part))
  }
}

pub struct DecibelBands {

}

impl fmt::Display for DecibelBands {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "TODO")
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitDecibelBands;

impl MetrifulUnit for UnitDecibelBands {
  type Output = DecibelBands;

  fn name() -> &'static str {
    "decibel bands"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

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

impl fmt::Display for SoundMeasurementStability {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", match self {
      SoundMeasurementStability::Stable => "stable",
      SoundMeasurementStability::Unstable => "unstable"
    })
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitSoundMeasurementStability;

impl MetrifulUnit for UnitSoundMeasurementStability {
  type Output = SoundMeasurementStability;

  fn name() -> &'static str {
    "sound measurement stability"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    match device.smbus_read_byte_data(register)? {
      1 => Ok(SoundMeasurementStability::Stable),
      _ => Ok(SoundMeasurementStability::Unstable),
    }
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitIlluminance;

impl MetrifulUnit for UnitIlluminance {
  type Output = f32;

  fn name() -> &'static str {
    "lux"
  }

  fn symbol() -> Option<&'static str> {
    Some("lx")
  }

  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, 3)?);
    let uint_part = bytes.get_u16_le();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(uint_part, frac_part))
  }
}

