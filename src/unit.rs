use std::convert::TryInto;
use std::fmt;

use bytes::{Bytes, Buf};
use chrono::{DateTime, Utc};
use i2cdev::core::I2CDevice;
use i2cdev::linux::LinuxI2CDevice;

#[cfg(feature = "serde")] use chrono::SecondsFormat;
#[cfg(feature = "serde")] use serde::{Serialize, ser::{Serializer, SerializeStruct}};

use crate::error::*;
use crate::metric::*;
use crate::util::*;

/// A combined unit and value, generally the result of a metric read.
///
/// Note that the various "combined read" metrics will contain structs with
/// nested `UnitValue`s in their respective `value` field.
///
/// All `UnitValue` instances 
#[derive(Debug, Clone)]
pub struct UnitValue<U> where U: MetrifulUnit {
  /// The unit of the read value, including name and symbol.
  ///
  /// Units also have utility functions for formatting values in a
  /// human-readable, type-specific manner.
  pub unit: U,

  /// The read value in its native datatype
  pub value: U::Output,
  
  /// The system time (UTC) when the metric was read by the library.
  pub time: DateTime<Utc>,
}

impl<U> UnitValue<U> where U: MetrifulUnit {
  fn from_bytes(bytes: &mut Bytes) -> Result<Self> {
    Ok(UnitValue {
      unit: U::default(),
      value: U::from_bytes(bytes)?,
      time: Utc::now(),
    })
  }
}

impl<U> fmt::Display for UnitValue<U> where U: MetrifulUnit {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", U::format_value(&self.value))
  }
}

#[cfg(feature = "serde")]
impl<U> Serialize for UnitValue<U> where U: MetrifulUnit {
  fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
  where
      S: Serializer
  {
    let mut state = serializer.serialize_struct("UnitValue", 5)?;
    state.serialize_field("timestamp", &self.time.to_rfc3339_opts(SecondsFormat::Secs, true))?;
    state.serialize_field("unit_name", U::name())?;
    state.serialize_field("unit_symbol", &U::symbol())?;
    state.serialize_field("value", &self.value)?;
    state.serialize_field("formatted_value", &U::format_value(&self.value))?;
    state.end()
  }
}

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

pub trait MetrifulUnit: Sized + Default + fmt::Debug + Copy + Clone + Send + Sync {
  /// This unit's native datatype.
  #[cfg(feature = "serde")] type Output: fmt::Display + fmt::Debug + Serialize + Send + Sync;
  #[cfg(not(feature = "serde"))] type Output: fmt::Display + fmt::Debug + Send + Sync;

  /// The human-readable name of the unit
  fn name() -> &'static str;

  /// Instance-accessible `name()`
  fn get_name(&self) -> &'static str {
    Self::name()
  }

  /// The human-readable symbol for this unit
  fn symbol() -> Option<&'static str>;

  /// Instance-accessible `symbol()`
  fn get_symbol(&self) -> Option<&'static str> {
    Self::symbol()
  }

  fn format_value(value: &Self::Output) -> String {
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
    "\u{2103}".into()
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
    "ohms"
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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CombinedAirData {
  pub temperature: UnitValue<UnitDegreesCelsius>,
  pub pressure: UnitValue<UnitPascals>,
  pub humidity: UnitValue<UnitRelativeHumidity>,
  pub gas_sensor_resistance: UnitValue<UnitResistance>,
}

impl fmt::Display for CombinedAirData {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "temperature:           {}", self.temperature)?;
    writeln!(f, "pressure:              {}", self.pressure)?;
    writeln!(f, "humidity:              {}", self.humidity)?;
    writeln!(f, "gas sensor resistance: {}", self.gas_sensor_resistance)?;

    Ok(())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitCombinedAirData;

impl MetrifulUnit for UnitCombinedAirData {
  type Output = CombinedAirData;

  fn name() -> &'static str {
    "combined air data"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    12
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let temperature = UnitValue::<UnitDegreesCelsius>::from_bytes(bytes)?;
    let pressure = UnitValue::<UnitPascals>::from_bytes(bytes)?;
    let humidity = UnitValue::<UnitRelativeHumidity>::from_bytes(bytes)?;
    let gas_sensor_resistance = UnitValue::<UnitResistance>::from_bytes(bytes)?;

    Ok(CombinedAirData {
      temperature,
      pressure,
      humidity,
      gas_sensor_resistance,
    })
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
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase"))]
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

  pub fn to_uint(&self) -> u8 {
    match self {
      AQIAccuracy::Invalid => 0,
      AQIAccuracy::Low => 1,
      AQIAccuracy::Medium => 2,
      AQIAccuracy::High => 3
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
    "AQI accuracy"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    1
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    AQIAccuracy::from_byte(bytes.get_u8())
  }
}

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CombinedAirQualityData {
  pub aqi: UnitValue<UnitAirQualityIndex>,
  pub estimated_co2: UnitValue<UnitPartsPerMillion>,
  pub estimated_voc: UnitValue<UnitPartsPerMillion>,
  pub aqi_accuracy: UnitValue<UnitAQIAccuracy>,
}

impl fmt::Display for CombinedAirQualityData {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "air quality index: {}", self.aqi)?;
    writeln!(f, "estimated CO2:     {}", self.estimated_co2)?;
    writeln!(f, "estimated VOCs:    {}", self.estimated_voc)?;
    writeln!(f, "AQI accuracy:      {}", self.aqi_accuracy)?;

    Ok(())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitCombinedAirQualityData;

impl MetrifulUnit for UnitCombinedAirQualityData {
  type Output = CombinedAirQualityData;

  fn name() -> &'static str {
    "combined air quality data"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    10
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let aqi = UnitValue::<UnitAirQualityIndex>::from_bytes(bytes)?;
    let estimated_co2 = UnitValue::<UnitPartsPerMillion>::from_bytes(bytes)?;
    let estimated_voc = UnitValue::<UnitPartsPerMillion>::from_bytes(bytes)?;
    let aqi_accuracy = UnitValue::<UnitAQIAccuracy>::from_bytes(bytes)?;

    Ok(CombinedAirQualityData {
      aqi,
      estimated_co2,
      estimated_voc,
      aqi_accuracy,
    })
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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CombinedLightData {
  pub illuminance: UnitValue<UnitIlluminance>,
  pub white_level: UnitValue<UnitWhiteLevel>,
}

impl fmt::Display for CombinedLightData {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "illuminance: {}", self.illuminance)?;
    writeln!(f, "white level: {}", self.white_level)?;

    Ok(())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitCombinedLightData;

impl MetrifulUnit for UnitCombinedLightData {
  type Output = CombinedLightData;

  fn name() -> &'static str {
    "combined light data"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    5
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let illuminance = UnitValue::<UnitIlluminance>::from_bytes(bytes)?;
    let white_level = UnitValue::<UnitWhiteLevel>::from_bytes(bytes)?;

    Ok(CombinedLightData {
      illuminance,
      white_level,
    })
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitAWeightedSPL;

impl MetrifulUnit for UnitAWeightedSPL {
  type Output = f32;

  fn name() -> &'static str {
    "A-weighted sound pressure level"
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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct SPLFrequencyBands(pub [f32; 6]);

impl fmt::Display for SPLFrequencyBands {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{:?}", self.0)
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitSPLFrequencyBands;

impl MetrifulUnit for UnitSPLFrequencyBands {
  type Output = SPLFrequencyBands;

  fn name() -> &'static str {
    "sound pressure level frequency bands"
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

    Ok(SPLFrequencyBands(bands))
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
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase"))]
pub enum SoundMeasurementStability {
  /// Microphone initialization has finished
  Stable,

  /// Microphone initialization still ongoing
  Unstable
}

impl SoundMeasurementStability {
  /// Converts this value to an int: 0 (unstable), 1 (stable)
  pub fn to_uint(&self) -> u8 {
    match self {
      SoundMeasurementStability::Stable => 1,
      SoundMeasurementStability::Unstable => 0
    }
  }
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


#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CombinedSoundData {
  pub weighted_spl: UnitValue<UnitAWeightedSPL>,
  pub spl_bands: UnitValue<UnitSPLFrequencyBands>,
  pub peak_amplitude: UnitValue<UnitMillipascal>,
  pub measurement_stability: UnitValue<UnitSoundMeasurementStability>,
}

impl fmt::Display for CombinedSoundData {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "a-weighted SPL:        {}", self.weighted_spl)?;
    writeln!(f, "SPL frequency bands:   {}", self.spl_bands)?;
    writeln!(f, "peak amplitude:        {}", self.peak_amplitude)?;
    writeln!(f, "measurement stability: {}", self.measurement_stability)?;

    Ok(())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitCombinedSoundData;

impl MetrifulUnit for UnitCombinedSoundData {
  type Output = CombinedSoundData;

  fn name() -> &'static str {
    "combined sound data"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    18
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let weighted_spl = UnitValue::<UnitAWeightedSPL>::from_bytes(bytes)?;
    let spl_bands = UnitValue::<UnitSPLFrequencyBands>::from_bytes(bytes)?;
    let peak_amplitude = UnitValue::<UnitMillipascal>::from_bytes(bytes)?;
    let measurement_stability = UnitValue::<UnitSoundMeasurementStability>::from_bytes(bytes)?;

    Ok(CombinedSoundData {
      weighted_spl,
      spl_bands,
      peak_amplitude,
      measurement_stability,
    })
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
#[cfg_attr(feature = "serde", derive(Serialize))]
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
#[cfg_attr(feature = "serde", derive(Serialize), serde(rename_all = "lowercase"))]
pub enum ParticleDataValidity {
  /// Particle sensor is still initializing (or is not enabled)
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

#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CombinedParticleData {
  pub duty_cycle: UnitValue<UnitPercent>,
  pub concentration: UnitValue<UnitRawParticleConcentration>,
  pub validity: UnitValue<UnitParticleDataValidity>,
}

impl fmt::Display for CombinedParticleData {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(f, "duty cycle:    {}", self.duty_cycle)?;
    writeln!(f, "concentration: {}", self.concentration)?;
    writeln!(f, "validity:      {}", self.validity)?;

    Ok(())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitCombinedParticleData;

impl MetrifulUnit for UnitCombinedParticleData {
  type Output = CombinedParticleData;

  fn name() -> &'static str {
    "combined particle data"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    6
  }

  fn from_bytes(bytes: &mut Bytes) -> Result<Self::Output> {
    let duty_cycle = UnitValue::<UnitPercent>::from_bytes(bytes)?;
    let concentration = UnitValue::<UnitRawParticleConcentration>::from_bytes(bytes)?;
    let validity = UnitValue::<UnitParticleDataValidity>::from_bytes(bytes)?;

    Ok(CombinedParticleData {
      duty_cycle,
      concentration,
      validity,
    })
  }
}

/// All sensor data, read at once.
///
/// Note that air quality and particle data have additional requirements and may
/// be invalid; they will be marked as such.
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CombinedData {
  pub air: UnitValue<UnitCombinedAirData>,
  pub air_quality: UnitValue<UnitCombinedAirQualityData>,
  pub light: UnitValue<UnitCombinedLightData>,
  pub sound: UnitValue<UnitCombinedSoundData>,
  pub particle: UnitValue<UnitCombinedParticleData>,
}

impl fmt::Display for CombinedData {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    writeln!(
      f, "air data:\n{}",
      textwrap::indent(&self.air.value.to_string(), "  ")
    )?;

    writeln!(
      f, "air quality data:\n{}",
      textwrap::indent(&self.air_quality.value.to_string(), "  ")
    )?;

    writeln!(
      f, "light data:\n{}",
      textwrap::indent(&self.light.value.to_string(), "  ")
    )?;

    writeln!(
      f, "sound data:\n{}",
      textwrap::indent(&self.sound.value.to_string(), "  ")
    )?;

    writeln!(
      f, "particle data:\n{}",
      textwrap::indent(&self.particle.value.to_string(), "  ")
    )?;

    Ok(())
  }
}

#[derive(Default, Debug, Copy, Clone)]
pub struct UnitCombinedData;

impl MetrifulUnit for UnitCombinedData {
  type Output = CombinedData;

  fn name() -> &'static str {
    "all combined data"
  }

  fn symbol() -> Option<&'static str> {
    None
  }

  fn len() -> u8 {
    0
  }

  fn from_bytes(_bytes: &mut Bytes) -> Result<Self::Output> {
    Err(MetrifulError::InvalidCombinedDataFromBytes)
  }

  fn read(device: &mut LinuxI2CDevice, _register: u8) -> Result<Self::Output> {
    let air = METRIC_COMBINED_AIR_DATA.read(device)?;
    let air_quality = METRIC_COMBINED_AIR_QUALITY_DATA.read(device)?;
    let light = METRIC_COMBINED_LIGHT_DATA.read(device)?;
    let sound = METRIC_COMBINED_SOUND_DATA.read(device)?;
    let particle = METRIC_COMBINED_PARTICLE_DATA.read(device)?;

    Ok(CombinedData {
      air,
      air_quality,
      light,
      sound,
      particle,
    })
  }
}
