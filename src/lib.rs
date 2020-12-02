use std::{path::Path, fmt, thread, time::{Duration, Instant}};

use bytes::{Bytes, Buf};
use i2cdev::core::*;
use i2cdev::linux::LinuxI2CDevice;
use sysfs_gpio::{Direction, Pin};

pub mod error;
pub mod metric;
pub mod unit;
pub mod util;

use error::*;
use metric::*;
use unit::*;
use util::*;

/// Metriful i2c address. Note: 0x70 if solder bridge is closed.
pub const METRIFUL_ADDRESS: u16 = 0x71;

/// Measurement cycle period per
#[derive(Copy, Clone)]
pub enum CyclePeriod {
  Period0,
  Period1,
  Period2,
}

impl fmt::Debug for CyclePeriod {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_tuple("CyclePeriod")
      .field(&format!("{}s", self.to_duration().as_secs()))
      .finish()
  }
}

impl CyclePeriod {
  pub fn from_value(value: u8) -> Result<CyclePeriod> {
    match value {
      0 => Ok(CyclePeriod::Period0),
      1 => Ok(CyclePeriod::Period1),
      2 => Ok(CyclePeriod::Period2),
      _ => Err(MetrifulError::InvalidCyclePeriod(value))
    }
  }

  pub fn to_value(&self) -> u8 {
    match self {
      CyclePeriod::Period0 => 0,
      CyclePeriod::Period1 => 1,
      CyclePeriod::Period2 => 2,
    }
  }

  pub fn to_duration(&self) -> Duration {
    Duration::from_secs(match self {
      CyclePeriod::Period0 => 3,
      CyclePeriod::Period1 => 100,
      CyclePeriod::Period2 => 300,
    })
  }
}

#[derive(Debug, Copy, Clone)]
pub enum OperationalMode {
  Cycle(CyclePeriod),
  Standby
}

#[derive(Debug, Copy, Clone)]
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
pub enum InterruptStatus<T> {
  Disabled,
  Enabled(T),
}

#[derive(Debug, Copy, Clone)]
pub enum InterruptMode {
  Latch,
  Comparator
}

#[derive(Debug, Clone, Copy)]
pub enum InterruptPolarity {
  /// Interrupt triggers when n > threshold
  Positive,

  /// Interrupt triggers when n < threshold
  Negative
}

#[derive(Debug, Copy, Clone)]
pub struct SoundInterrupt {
  mode: InterruptMode,

  // note: sound interrupt is missing polarity per the docs, but register 0x88
  // is undocumented - maybe it exists?

  /// Interrupt threshold in mPa.
  // todo: unsigned?
  threshold: u16
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
pub struct LightInterrupt {
  mode: InterruptMode,

  /// Interrupt comparison polarity
  polarity: InterruptPolarity,

  /// Interrupt threshold in lux
  threshold: f32,
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
pub struct DeviceStatus {
  particle_sensor: ParticleSensorMode,
  light_int: InterruptStatus<LightInterrupt>,
  sound_int: InterruptStatus<SoundInterrupt>,
  mode: OperationalMode,
}

impl DeviceStatus {
  fn read(device: &mut LinuxI2CDevice) -> Result<DeviceStatus> {
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

pub struct Metriful {
  ready_pin: Pin,
  device: LinuxI2CDevice,

  status: Option<DeviceStatus>,
}

impl fmt::Debug for Metriful {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.debug_struct("Metriful")
      .field("ready_pin", &self.ready_pin)
      .field("status", &self.status)
      .finish()
  }
}

pub struct MetricReadIterator<'a, U> where U: MetrifulUnit {
  device: &'a mut Metriful,
  metric: Metric<U>,
  timeout: Option<Duration>,
  error: bool,
}

impl<'a, 'b, U> Iterator for MetricReadIterator<'a, U>
where
  U: MetrifulUnit
{
  type Item = Result<U::Output>;

  fn next(&mut self) -> Option<Self::Item> {
    // TODO: this doesn't actually wait for the cycle read to complete;
    // maybe we should implement our own loop read logic since it'd be fairly
    // ugly to use the builtin without constantly polling for interrupts
    if self.error {
      return None;
    }

    match self.device.wait_for_ready_timeout(self.timeout) {
      Ok(()) => (),
      Err(e) => {
        self.error = true;
        return Some(Err(e));
      }
    };

    match self.device.read(&self.metric) {
      Ok(result) => Some(Ok(result)),
      Err(e) => {
        self.error = true;
        Some(Err(e))
      }
    }
  }
}

impl Metriful {
  pub fn new(ready_pin: Pin, device: LinuxI2CDevice) -> Metriful {
    Metriful {
      ready_pin, device,
      status: None
    }
  }

  pub fn try_new(
    gpio_ready: u64,
    i2c_device: &Path,
    i2c_address: u16
  ) -> Result<Metriful> {
    let ready_pin = Pin::new(gpio_ready);
    ready_pin.export()?;
    ready_pin.set_active_low(false)?;
    ready_pin.set_direction(Direction::In)?;

    let device = LinuxI2CDevice::new(i2c_device, i2c_address)?;

    Ok(Metriful {
      ready_pin,
      device,
      status: None
    })
  }

  /// Returns true if the sensor's ready pin is asserted.
  pub fn is_ready(&self) -> Result<bool> {
    Ok(self.ready_pin.get_value()? == 0)
  }

  /// Sleeps the thread until `Metriful::is_ready()` returns true, polling every
  /// 100ms. If a timeout is set and exceeded, returns an error.
  pub fn wait_for_ready_timeout(&self, timeout: Option<Duration>) -> Result<()> {
    let start = Instant::now();

    loop {
      if self.is_ready()? {
        return Ok(());
      }

      if let Some(timeout) = timeout {
        if start.elapsed() > timeout {
          return Err(MetrifulError::ReadyTimeoutExceeded)
        } else {
          thread::sleep(Duration::from_millis(100));
        }
      }
    }
  }

  /// Waits for `Metriful::is_ready()` to become true and executes the given
  /// function. If the timeout is exceeded, an error is returned.
  pub fn execute_when_ready_timeout<T>(
    &mut self,
    func: impl FnOnce(&mut Metriful) -> T,
    timeout: Option<Duration>,
  ) -> Result<T> {
    let start = Instant::now();

    loop {
      if self.is_ready()? {
        return Ok(func(self));
      }

      if let Some(timeout) = timeout {
        if start.elapsed() > timeout {
          return Err(MetrifulError::ReadyTimeoutExceeded)
        } else {
          thread::sleep(Duration::from_millis(100));
        }
      }
    }
  }

  pub fn execute_when_ready<T>(
    &mut self,
    func: impl FnOnce(&mut Metriful) -> T,
  ) -> Result<T> {
    self.execute_when_ready_timeout(func, None)
  }

  pub fn reset(&mut self) -> Result<()> {
    self.device.smbus_write_byte(0xE2)?;

    Ok(())
  }

  pub fn read<U: MetrifulUnit>(&mut self, metric: &Metric<U>) -> Result<U::Output> {
    metric.read(&mut self.device)
  }

  pub fn cycle_read_iter_timeout<'a, U>(
    &'a mut self,
    metric: Metric<U>,
    timeout: Option<Duration>,
  ) -> MetricReadIterator<U>
  where
    U: MetrifulUnit
  {
    MetricReadIterator {
      device: self,
      error: false,
      metric,
      timeout,
    }
  }

  pub fn cycle_read_iter<'a, U>(
    &'a mut self,
    metric: Metric<U>,
  ) -> MetricReadIterator<U>
  where
    U: MetrifulUnit
  {
    MetricReadIterator {
      device: self,
      error: false,
      timeout: None,
      metric,
    }
  }

  pub fn read_status(&mut self) -> Result<DeviceStatus> {
    let status = DeviceStatus::read(&mut self.device)?;
    self.status = Some(status.clone());

    Ok(status)
  }
}
