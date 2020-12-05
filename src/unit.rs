use std::convert::TryInto;
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

pub trait MetrifulUnit: Sized + Default + fmt::Debug + Copy + Clone {
  /// This unit's native datatype.
  type Output: fmt::Display + fmt::Debug;

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

  /// Length of this datatype in bytes
  fn len() -> u8;

  /// Reads this datatype from raw bytes
  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output>;

  /// Reads the appropriate value for this unit from the given register.
  fn read(device: &mut LinuxI2CDevice, register: u8) -> Result<Self::Output> {
    let mut bytes = Bytes::from(device.smbus_read_i2c_block_data(register, Self::len())?);
    Self::from_bytes(&mut bytes)
  }

  fn new_metric(register: u8) -> Metric<Self> {
    Metric {
      register,
      unit: Self::default()
    }
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitDegreesCelsius;

impl MetrifulUnit for UnitDegreesCelsius {
  type Output = f32;

  fn name() -> &'static str {
    "degrees Celsius"
  }

  fn symbol() -> Option<&'static str> {
    "\u{2103}C".into()
  }

  fn len() -> u8 {
    2
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let int_part = bytes.get_i8();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(int_part, frac_part))
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitPascals;

impl MetrifulUnit for UnitPascals {
  type Output = u32;

  fn name() -> &'static str {
    "pascals"
  }

  fn symbol() -> Option<&'static str> {
    Some("Pa")
  }

  fn len() -> u8 {
    4
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    Ok(bytes.get_u32_le())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitRelativeHumidity;

impl MetrifulUnit for UnitRelativeHumidity {
  type Output = f32;

  fn name() -> &'static str {
    "% relative humidity"
  }

  fn symbol() -> Option<&'static str> {
    Some("% RH")
  }

  fn len() -> u8 {
    2
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let int_part = bytes.get_u8();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(int_part, frac_part))
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitResistance;

impl MetrifulUnit for UnitResistance {
  type Output = u32;

  fn name() -> &'static str {
    "resistance"
  }

  fn symbol() -> Option<&'static str> {
    Some("Ω")
  }

  fn len() -> u8 {
    4
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    Ok(bytes.get_u32_le())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitAirQualityIndex;

impl MetrifulUnit for UnitAirQualityIndex {
  type Output = f32;

  fn name() -> &'static str {
    "AQI"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    3
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let int_part = bytes.get_u16_le();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(int_part, frac_part))
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitPartsPerMillion;

impl MetrifulUnit for UnitPartsPerMillion {
  type Output = f32;

  fn name() -> &'static str {
    "parts per million"
  }

  fn symbol() -> Option<&'static str> {
    Some("ppm")
  }

  fn len() -> u8 {
    3
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let int_part = bytes.get_u16_le();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(int_part, frac_part))
  }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum AQIAccuracy {
  Invalid,
  Low,
  Medium,
  High
}

impl AQIAccuracy {
  pub fn from_byte(byte: u8) -> Result<AQIAccuracy> {
    match byte {
      0 => Ok(AQIAccuracy::Invalid),
      1 => Ok(AQIAccuracy::Low),
      2 => Ok(AQIAccuracy::Medium),
      3 => Ok(AQIAccuracy::High,),
      _ => Err(MetrifulError::InvalidAQIAccuracy(byte))
    }
  }
}

impl fmt::Display for AQIAccuracy {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", match self {
      AQIAccuracy::Invalid => "invalid",
      AQIAccuracy::Low => "low",
      AQIAccuracy::Medium => "medium",
      AQIAccuracy::High => "high",
    })
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitAQIAccuracy;

impl MetrifulUnit for UnitAQIAccuracy {
  type Output = AQIAccuracy;

  fn name() -> &'static str {
    "parts per million"
  }

  fn symbol() -> Option<&'static str> {
    Some("ppm")
  }

  fn len() -> u8 {
    1
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    AQIAccuracy::from_byte(bytes.get_u8())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitAWeightedDecibels;

impl MetrifulUnit for UnitAWeightedDecibels {
  type Output = f32;

  fn name() -> &'static str {
    "A-weighted decibels"
  }

  fn symbol() -> Option<&'static str> {
    Some("dBa")
  }

  fn len() -> u8 {
    2
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let uint_part = bytes.get_u8();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(uint_part, frac_part))
  }
}

#[derive(Debug)]
pub struct DecibelBands([f32; 6]);

impl fmt::Display for DecibelBands {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{:?}", self.0)
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

  fn len() -> u8 {
    12
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let int_parts = &bytes[0..6];
    let frac_parts = &bytes[6..12];

    let bands: [f32; 6] = int_parts.iter()
      .copied()
      .zip(frac_parts.iter().copied())
      .map(|(int_part, frac_part)| read_f32_with_u8_denom(int_part, frac_part))
      .collect::<Vec<_>>()
      .try_into()
      .map_err(|_| MetrifulError::DecibelBandsError)?;

    Ok(DecibelBands(bands))
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

  fn len() -> u8 {
    3
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let uint_part = bytes.get_u16_le();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(uint_part, frac_part))
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitWhiteLevel;

impl MetrifulUnit for UnitWhiteLevel {
  type Output = u16;

  fn name() -> &'static str {
    "white level"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    2
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    Ok(bytes.get_u16_le())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitMillipascal;

impl MetrifulUnit for UnitMillipascal {
  type Output = f32;

  fn name() -> &'static str {
    "millipascals"
  }

  fn symbol() -> Option<&'static str> {
    Some("mPa")
  }

  fn len() -> u8 {
    3
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let uint_part = bytes.get_u16_le();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(uint_part, frac_part))
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

  fn len() -> u8 {
    1
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    match bytes.get_u8() {
      1 => Ok(SoundMeasurementStability::Stable),
      _ => Ok(SoundMeasurementStability::Unstable),
    }
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitPercent;

impl MetrifulUnit for UnitPercent {
  type Output = f32;

  fn name() -> &'static str {
    "percent"
  }

  fn symbol() -> Option<&'static str> {
    Some("%")
  }

  fn len() -> u8 {
    2
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let uint_part = bytes.get_u8();
    let frac_part = bytes.get_u8();

    Ok(read_f32_with_u8_denom(uint_part, frac_part))
  }
}

/// Raw particle concentration from attached particle sensor. Underlying
/// datatype varies depending on sensor attached.
///
/// Both values are always set and should be approximately equal.
#[derive(Debug, Copy, Clone)]
pub struct RawParticleConcentration {
  /// 16-bit integer with two-digit fractional part; micrograms per cubic meter
  pub sds011_value: f32,

  /// 16 bit integer; particles per liter
  pub ppd42_value: u16,
}

impl PartialEq for RawParticleConcentration {
  fn eq(&self, other: &Self) -> bool {
    self.ppd42_value == other.ppd42_value
  }
}

impl Eq for RawParticleConcentration {}

impl Ord for RawParticleConcentration {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    self.ppd42_value.cmp(&other.ppd42_value)
  }
}

impl PartialOrd for RawParticleConcentration {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl fmt::Display for RawParticleConcentration {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.sds011_value)
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitRawParticleConcentration;

impl MetrifulUnit for UnitRawParticleConcentration {
  type Output = RawParticleConcentration;

  fn name() -> &'static str {
    "raw particle concentration"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    3
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let uint_part = bytes.get_u16_le();
    let frac_part = bytes.get_u8();

    Ok(RawParticleConcentration {
      sds011_value: read_f32_with_u8_denom(uint_part, frac_part),
      ppd42_value: uint_part
    })
  }
}

#[derive(Debug, Copy, Clone, Ord, PartialOrd, Eq, PartialEq)]
pub enum ParticleDataValidity {
  /// Particle sensor is still initializing
  Initializing,

  /// Particle sensor data is "likely to have settled"
  Settled,
}

impl ParticleDataValidity {
  pub fn from_byte(byte: u8) -> Result<ParticleDataValidity> {
    match byte {
      0 => Ok(ParticleDataValidity::Initializing),
      1 => Ok(ParticleDataValidity::Settled),
      _ => Err(MetrifulError::InvalidParticleDataValidity(byte))
    }
  }
}

impl fmt::Display for ParticleDataValidity {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", match self {
      ParticleDataValidity::Initializing => "initializing",
      ParticleDataValidity::Settled => "settled",
    })
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitParticleDataValidity;

impl MetrifulUnit for UnitParticleDataValidity {
  type Output = ParticleDataValidity;

  fn name() -> &'static str {
    "particle data validity"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    1
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    Ok(ParticleDataValidity::from_byte(bytes.get_u8())?)
  }
}
