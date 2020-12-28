use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use bytes::{Bytes, Buf};
use i2cdev::core::*;
use i2cdev::linux::LinuxI2CDevice;

#[cfg(feature = "serde")] use serde::{Serialize, ser::{Serializer, SerializeStruct}};

use super::error::*;
use super::util::*;

/// Supported measurement cycles built in to the MS430.
#[derive(Copy, Clone, PartialEq, Ord, PartialOrd, Eq)]
pub enum CyclePeriod {
  /// Period `0`, i.e. 3 second cycles
  Period0,

  /// Period `1`, i.e. 100 second cycles
  Period1,

  /// Period `2`, i.e. 300 second cycles
  Period2,
}

impl FromStr for CyclePeriod {
  type Err = MetrifulError;

  fn from_str(s: &str) -> Result<Self> {
    match s {
      "0" | "3s" => Ok(CyclePeriod::Period0),
      "1" | "100s" => Ok(CyclePeriod::Period1),
      "2" | "300s" => Ok(CyclePeriod::Period2),
      other => Err(MetrifulError::InvalidCyclePeriodString(other.to_string()))
    }
  }
}

#[cfg(feature = "serde")]
impl Serialize for CyclePeriod {
  fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
  where
      S: Serializer
  {
    let mut state = serializer.serialize_struct("CyclePeriod", 1)?;
    state.serialize_field("period", &format!("{:?}", self.to_duration()))?;
    state.end()
  }
}

impl fmt::Debug for CyclePeriod {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("CyclePeriod")
      .field(&format!("{}s", self.to_duration().as_secs()))
      .finish()
  }
}

impl CyclePeriod {
  /// Returns a CyclePeriod for a given Metriful register value.
  pub fn from_value(value: u8) -> Result<CyclePeriod> {
    match value {
      0 => Ok(CyclePeriod::Period0),
      1 => Ok(CyclePeriod::Period1),
      2 => Ok(CyclePeriod::Period2),
      _ => Err(MetrifulError::InvalidCyclePeriod(value))
    }
  }

  /// Returns the metriful register value for this period, one of 0x0, 0x1, or
  /// 0x2.
  pub fn to_value(&self) -> u8 {
    match self {
      CyclePeriod::Period0 => 0,
      CyclePeriod::Period1 => 1,
      CyclePeriod::Period2 => 2,
    }
  }

  /// Returns a Duration for this CyclePeriod.
  pub fn to_duration(&self) -> Duration {
    Duration::from_secs(match self {
      CyclePeriod::Period0 => 3,
      CyclePeriod::Period1 => 100,
      CyclePeriod::Period2 => 300,
    })
  }
}

/// Device operational mode.
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase", tag = "mode"))]
pub enum OperationalMode {
  Cycle(CyclePeriod),
  Standby
}

impl OperationalMode {
  /// Determines if it is valid to switch to this mode from the given previous
  /// mode.
  pub fn is_switch_allowed(&self, from: OperationalMode) -> bool {
    match self {
      OperationalMode::Standby => !matches!(from, OperationalMode::Standby),
      OperationalMode::Cycle(_) => !matches!(from, OperationalMode::Cycle(_))
    }
  }

  /// Returns the maximum expected time for the sensor to return to READY state
  /// after switching to this operational mode from its opposite mode.
  pub fn ready_duration(&self) -> Duration {
    match self {
      OperationalMode::Standby => Duration::from_millis(11),
      OperationalMode::Cycle(CyclePeriod::Period0) => Duration::from_millis(600),
      OperationalMode::Cycle(_) => Duration::from_millis(2600),
    }
  }
}

#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "kebab-case"))]
pub enum ParticleSensorMode {
  Disabled,
  EnabledPPD42,
  EnabledSDS011,
}

impl ParticleSensorMode {
  pub fn from_value(value: u8) -> Result<ParticleSensorMode> {
    match value {
      0 => Ok(ParticleSensorMode::Disabled),
      1 => Ok(ParticleSensorMode::EnabledPPD42),
      2 => Ok(ParticleSensorMode::EnabledSDS011),
      _ => Err(MetrifulError::InvalidParticleSensorMode(value)),
    }
  }

  pub fn to_value(&self) -> u8 {
    match self {
      ParticleSensorMode::Disabled => 0,
      ParticleSensorMode::EnabledPPD42 => 1,
      ParticleSensorMode::EnabledSDS011 => 2,
    }
  }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase", tag = "status"))]
pub enum InterruptStatus<T> {
  Disabled,
  Enabled(T),
}

#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase"))]
pub enum InterruptMode {
  Latch,
  Comparator
}

#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase"))]
pub enum InterruptPolarity {
  /// Interrupt triggers when n > threshold
  Positive,

  /// Interrupt triggers when n < threshold
  Negative
}

#[derive(Debug, Copy, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SoundInterrupt {
  pub mode: InterruptMode,

  // note: sound interrupt is missing polarity per the docs, but register 0x88
  // is undocumented - maybe it exists?

  /// Interrupt threshold in mPa.
  // todo: unsigned?
  pub threshold: u16
}

impl SoundInterrupt {
  pub fn read(device: &mut LinuxI2CDevice) -> Result<SoundInterrupt> {
    let mode = match device.smbus_read_byte_data(0x87)? {
      0 => InterruptMode::Latch,
      _ => InterruptMode::Comparator,
    };

    let mut threshold_bytes = Bytes::from(device.smbus_read_i2c_block_data(0x86, 2)?);
    Ok(SoundInterrupt {
      mode,
      threshold: threshold_bytes.get_u16_le()
    })
  }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct LightInterrupt {
  pub mode: InterruptMode,

  /// Interrupt comparison polarity
  pub polarity: InterruptPolarity,

  /// Interrupt threshold in lux
  pub threshold: f32,
}

impl LightInterrupt {
  pub fn read(device: &mut LinuxI2CDevice) -> Result<LightInterrupt> {
    let mode = match device.smbus_read_byte_data(0x83)? {
      0 => InterruptMode::Latch,
      _ => InterruptMode::Comparator,
    };

    let polarity = match device.smbus_read_byte_data(0x84)? {
      0 => InterruptPolarity::Positive,
      _ => InterruptPolarity::Negative,
    };

    let mut threshold_bytes = Bytes::from(device.smbus_read_i2c_block_data(0x82, 3)?);
    let threshold = read_f32_with_u8_denom(
      threshold_bytes.get_u16_le(),
      threshold_bytes.get_u8()
    );

    Ok(LightInterrupt {
      mode,
      polarity,
      threshold,
    })
  }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase"))]
pub struct DeviceStatus {
  pub particle_sensor: ParticleSensorMode,
  pub light_int: InterruptStatus<LightInterrupt>,
  pub sound_int: InterruptStatus<SoundInterrupt>,
  pub mode: OperationalMode,
}

impl DeviceStatus {
  pub fn read(device: &mut LinuxI2CDevice) -> Result<DeviceStatus> {
    let particle_sensor = ParticleSensorMode::from_value(
      device.smbus_read_byte_data(0x07)?
    )?;

    let light_int = match device.smbus_read_byte_data(0x81)? {
      0 => InterruptStatus::Disabled,
      _ => InterruptStatus::Enabled(LightInterrupt::read(device)?),
    };

    let sound_int = match device.smbus_read_byte_data(0x86)? {
      0 => InterruptStatus::Disabled,
      _ => InterruptStatus::Enabled(SoundInterrupt::read(device)?)
    };

    let mode = match device.smbus_read_byte_data(0x8A)? {
      0 => OperationalMode::Standby,
      1 => OperationalMode::Cycle(
        CyclePeriod::from_value(device.smbus_read_byte_data(0x89)?)?
      ),
      byte => return Err(MetrifulError::InvalidOperationalMode(byte))
    };

    Ok(DeviceStatus {
      particle_sensor,
      light_int,
      sound_int,
      mode,
    })
  }
}
