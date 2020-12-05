use i2cdev::linux::LinuxI2CDevice;
use lazy_static::lazy_static;

use crate::error::*;
use crate::unit::*;

#[derive(Debug, Copy, Clone)]
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
  /// Temperature in degrees Celsius
  pub static ref METRIC_TEMPERATURE: Metric<UnitDegreesCelsius> = metric(0x21);

  /// Pressure in Pascals (Pa)
  pub static ref METRIC_PRESSURE: Metric<UnitPascals> = metric(0x22);

  /// Relative humidity percentage
  pub static ref METRIC_RELATIVE_HUMIDITY: Metric<UnitRelativeHumidity> = metric(0x23);

  /// Gas sensor resistance
  pub static ref METRIC_GAS_RESISTANCE: Metric<UnitResistance> = metric(0x24);

  /// Air quality index
  pub static ref METRIC_AQI: Metric<UnitAirQualityIndex> = metric(0x25);

  /// Estimated CO2 concentration (based on gas sensor)
  pub static ref METRIC_EST_CO2: Metric<UnitPartsPerMillion> = metric(0x26);

  /// "Equivalent breath" VOC concentration
  pub static ref METRIC_VOC: Metric<UnitPartsPerMillion> = metric(0x27);

  /// AQI accuracy indicator
  pub static ref METRIC_AQI_ACCURACY: Metric<UnitAQIAccuracy> = metric(0x28);

  /// Illuminance in lux
  pub static ref METRIC_ILLUMINANCE: Metric<UnitIlluminance> = metric(0x31);

  /// White light level
  pub static ref METRIC_WHITE_LIGHT_LEVEL: Metric<UnitWhiteLevel> = metric(0x32);

  /// A-weighted sound pressure level in dBa
  pub static ref METRIC_WEIGHTED_SOUND_LEVEL: Metric<UnitAWeightedDecibels> = metric(0x41);

  /// Sound pressure level by frequency band
  pub static ref METRIC_SOUND_LEVEL: Metric<UnitDecibelBands> = metric(0x42);

  /// Measured peak sound amplitude "since last read"
  pub static ref METRIC_PEAK_SOUND_AMPLITUDE: Metric<UnitMillipascal> = metric(0x43);

  /// Self assessment of sound measurement stability
  pub static ref METRIC_SOUND_MEASUREMENT_STABILITY: Metric<UnitSoundMeasurementStability> = metric(0x44);

  /// Particle sensor duty cycle
  pub static ref METRIC_PARTICLE_SENSOR_DUTY_CYCLE: Metric<UnitPercent> = metric(0x51);

  /// Particle concentration as measured by external sensor
  pub static ref METRIC_PARTICLE_CONCENTRATION: Metric<UnitRawParticleConcentration> = metric(0x52);

  /// Self assessment of state of particle sensor, if attached
  pub static ref METRIC_PARTICLE_DATA_VALID: Metric<UnitParticleDataValidity> = metric(0x53);
}
