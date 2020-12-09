use std::{path::Path, fmt, thread, time::{Duration, Instant}};

use bytes::{Bytes, Buf};
use i2cdev::core::*;
use i2cdev::linux::LinuxI2CDevice;
use log::{trace, debug, info};
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

pub const READY_POLL_INTERVAL: u64 = 10;

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

/// An iterator for repeatedly collecting on-demand measurements.
///
/// Unless otherwise limited (e.g. `.take(n)`) this iterator will return results
/// forever. If an error occurs, it is returned as the next result and the
/// iterator terminates.
///
/// Each read takes approximately `interval`; intervals should be at least 2
/// seconds to ensure valid results, though smaller values may still be used.
/// Note that the device takes roughly 550ms to collect metrics, during which
/// the thread is blocked, effectively ensuring a minimum interval of 550ms.
/// The blocking time is automatically adjusted to ensure a consistent read
/// interval, though if more than `interval` time passes between subsequent
/// reads the next result will be fetched immediately and will only block until
/// the device becomes ready again in approximately 550ms.
///
/// Additionally, note that these on-demand measurements do not include air
/// quality data; these are only valid in cycle read mode.
pub struct MetricReadIterator<'a, U> where U: MetrifulUnit {
  device: &'a mut Metriful,
  metric: Metric<U>,
  interval: Duration,
  timeout: Option<Duration>,
  last_instant: Instant,
  error: bool,
}

impl<'a, U> Iterator for MetricReadIterator<'a, U>
where
  U: MetrifulUnit
{
  type Item = Result<UnitValue<U>>;

  fn next(&mut self) -> Option<Self::Item> {
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

    // attempt to correct any time variation < interval
    // if we exceed it, oh well
    let elapsed = self.last_instant.elapsed();
    if elapsed < self.interval {
      thread::sleep(self.interval - elapsed);
    }
    self.last_instant = Instant::now();

    let res = self.device.execute_measurement()
      .and_then(|()| self.device.wait_for_ready_timeout(self.timeout))
      .and_then(|()| self.device.read(self.metric));

    let ret = match res {
      Ok(result) => Some(Ok(result)),
      Err(e) => {
        self.error = true;
        Some(Err(e))
      }
    };

    ret
  }
}

/// An iterator that periodically returns results in cycle mode.
///
/// If the device is not in the proper cycle mode on the first call to
/// `.next()`, a mode change is executed per `Metriful::set_mode_timeout()`.
/// This may result up to 2 mode changes if the device is currently in a
/// different cycle mode, and may cause some delay (between ~0.6 and ~2.6
/// seconds) before the first read completes.
///
/// Unlike `MetricReadIterator`, this iterator makes no attempt to ensure a
/// consistent read interval and is entirely dependent on the sensor and GPIO
/// values. In particular, the first read should be expected to return
/// relatively quickly (2.6s in the 100s/300s interval cases), however
/// subsequent reads should be expected to take the full interval of time.
///
/// Note that subsequent calls to `.next()` must be made before the current
/// cycle ends or a measurement will be skipped. In the worst case, this means
/// callers have up to 2.95s (per the datasheet) to process a result and call
/// `.next()` again.
pub struct CycleReadIterator<'a, U> where U: MetrifulUnit {
  device: &'a mut Metriful,
  cycle_period: CyclePeriod,
  metric: Metric<U>,
  timeout: Option<Duration>,

  first: bool,
  error: bool,
}

impl<'a, U> Iterator for CycleReadIterator<'a, U> where U: MetrifulUnit {
  type Item = Result<UnitValue<U>>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.error {
      return None;
    }

    if self.first {
      match self.device.set_mode_timeout(OperationalMode::Cycle(self.cycle_period), self.timeout) {
        Ok(_) => {
          self.first = false;

          match self.device.read(self.metric) {
            Ok(res) => Some(Ok(res)),
            Err(e) => {
              self.error = true;
              Some(Err(e))
            }
          }
        },
        Err(e) => {
          self.error = true;
          return Some(Err(e));
        }
      }
    } else {
      let res = self.device.wait_for_not_ready_timeout(self.timeout)
        .and_then(|()| self.device.wait_for_ready_timeout(self.timeout))
        .and_then(|()| self.device.read(self.metric));

      match res {
        Ok(result) => Some(Ok(result)),
        Err(e) => {
          self.error = true;
          Some(Err(e))
        }
      }
    }
  }
}

impl Metriful {
  /// Creates a new Metriful given a preexisting GPIO `Pin` and
  /// `LinuxI2CDevice`. This ensures the device is ready and fetches the current
  /// state. Returns an error if the timeout is set and exceeded, or if device
  /// status cannot be read.
  ///
  /// Note that this does not reset the device. The manual recommends doing so
  /// before use; call `Metriful::reset()` to do so.
  pub fn try_new_device_timeout(
    ready_pin: Pin,
    device: LinuxI2CDevice,
    timeout: Option<Duration>,
  ) -> Result<Metriful> {
    trace!("Metriful::try_new_device_timeout(.., {:?})", timeout);

    let mut ret = Metriful {
      ready_pin, device,
      status: None
    };

    ret.wait_for_ready_timeout(timeout)?;
    ret.read_status()?;

    Ok(ret)
  }

  /// Initializes a new Metriful instance and fetches the current device status.
  /// Returns an error if the device does not become ready within the configured
  /// timeout or if current status cannot be read.
  ///
  /// Note that this does not reset the device. The manual recommends doing so
  /// before use; call `Metriful::reset()` to do so.
  pub fn try_new_timeout(
    gpio_ready: u64,
    i2c_device: &Path,
    i2c_address: u16,
    timeout: Option<Duration>
  ) -> Result<Metriful> {
    trace!(
      "Metriful::try_new_timeout({}, {}, {:x}, {:?})",
      gpio_ready, i2c_device.display(), i2c_address, timeout
    );

    let ready_pin = Pin::new(gpio_ready);
    ready_pin.export()?;
    ready_pin.set_active_low(false)?;
    ready_pin.set_direction(Direction::In)?;

    let device = LinuxI2CDevice::new(i2c_device, i2c_address)?;

    let mut ret = Metriful {
      ready_pin,
      device,
      status: None
    };

    ret.wait_for_ready_timeout(timeout)?;
    ret.read_status()?;

    Ok(ret)
  }

  /// Initializes a new Metriful instance and fetches the current device status.
  /// Returns an error if device status cannot be read. May block indefinitely
  /// if the device does not become ready.
  ///
  /// Note that this does not reset the device. The manual recommends doing so
  /// before use; call `Metriful::reset()` to do so.
  pub fn try_new(
    gpio_ready: u64,
    i2c_device: &Path,
    i2c_address: u16
  ) -> Result<Metriful> {
    Metriful::try_new_timeout(gpio_ready, i2c_device, i2c_address, None)
  }

  /// Returns true if the sensor's ready pin is asserted.
  pub fn is_ready(&self) -> Result<bool> {
    Ok(self.ready_pin.get_value()? == 0)
  }

  /// Returns true if the device is known to be in standby mode.
  ///
  /// If the device status is missing or outdated it may return false.
  pub fn is_mode_standby(&self) -> bool {
    if let Some(status) = &self.status {
      matches!(status.mode, OperationalMode::Standby)
    } else {
      false
    }
  }

  /// Returns true if the device is known to be in some cycle mode.
  ///
  /// If the device status is missing or outdated it may return false.
  pub fn is_mode_cycle(&self) -> bool {
    if let Some(status) = &self.status {
      matches!(status.mode, OperationalMode::Cycle(_))
    } else {
      false
    }
  }

  /// Ensures the device is currently ready.
  pub fn ensure_ready(&self) -> Result<()> {
    if self.is_ready()? {
      Ok(())
    } else {
      return Err(MetrifulError::NotReady)
    }
  }

  /// Sleeps the thread until `Metriful::is_ready()` returns true, polling every
  /// 10ms. If a timeout is set and exceeded, returns an error.
  pub fn wait_for_ready_timeout(&self, timeout: Option<Duration>) -> Result<()> {
    let start = Instant::now();

    loop {
      if self.is_ready()? {
        trace!("Metriful::wait_for_ready_timeout({:?}): is ready after {:?}", timeout, start.elapsed());
        return Ok(());
      }

      if let Some(timeout) = timeout {
        if start.elapsed() > timeout {
          trace!("Metriful::wait_for_ready_timeout({:?}): timeout exceeded", timeout);
          return Err(MetrifulError::ReadyTimeoutExceeded)
        } else {
          thread::sleep(Duration::from_millis(READY_POLL_INTERVAL));
        }
      }
    }
  }

  /// Sleeps the thread until `Metriful::is_ready()` returns true, polling every
  /// 10ms. This has no timeout and will wait indefinitely; see
  /// `wait_for_ready_timeout` if a timeout is desired.
  pub fn wait_for_ready(&self) -> Result<()> {
    self.wait_for_ready_timeout(None)
  }

  /// The inverse of `wait_for_ready_timeout`, this waits until the device is
  /// explicitly not ready, useful for e.g. waiting for a new cycle period.
  pub fn wait_for_not_ready_timeout(&self, timeout: Option<Duration>) -> Result<()> {
    let start = Instant::now();

    loop {
      if !self.is_ready()? {
        trace!("Metriful::wait_for_not_ready_timeout({:?}): is not ready after {:?}", timeout, start.elapsed());
        return Ok(());
      }

      if let Some(timeout) = timeout {
        if start.elapsed() > timeout {
          trace!("Metriful::wait_for_not_ready_timeout({:?}): timeout exceeded", timeout);
          return Err(MetrifulError::ReadyTimeoutExceeded)
        } else {
          thread::sleep(Duration::from_millis(READY_POLL_INTERVAL));
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
          thread::sleep(Duration::from_millis(READY_POLL_INTERVAL));
        }
      }
    }
  }

  /// Waits for `Metriful::is_ready()` to become true and executes the given
  /// function. This has no timeout and may wait indefinitely.
  pub fn execute_when_ready<T>(
    &mut self,
    func: impl FnOnce(&mut Metriful) -> T,
  ) -> Result<T> {
    self.execute_when_ready_timeout(func, None)
  }

  /// Sends a device reset command, waits for it to become ready again, and
  /// returns a refreshed `DeviceStatus`. Raises an error if the device is not
  /// initially ready.
  pub fn reset(&mut self) -> Result<DeviceStatus> {
    self.ensure_ready()?;

    self.device.smbus_write_byte(0xE2)?;
    self.sleep_write();

    self.wait_for_ready()?;
    Ok(self.read_status()?)
  }

  /// Sends a 'clear light interrupt' command. Will raise an error if the device
  /// is not ready.
  pub fn clear_light_interrupt(&mut self) -> Result<()> {
    self.ensure_ready()?;

    self.device.smbus_write_byte(0xE6)?;
    self.sleep_write();

    Ok(())
  }

  /// Sends a 'clear sound interrupt' command. Will raise an error if the device
  /// is not ready.
  pub fn clear_sound_interrupt(&mut self) -> Result<()> {
    self.ensure_ready()?;

    self.device.smbus_write_byte(0xE7)?;
    self.sleep_write();

    Ok(())
  }

  /// Naively changes the device's operational mode. This function does not
  /// ensure the device is in a valid state beforehand and may send illegal
  /// commands, however it will not block the thread beyond the required 6ms
  /// wait between commands (when setting a cycle period).
  ///
  /// This does not ensure the READY pin is asserted, nor does it ensure the
  /// given operational mode can be set directly, as changing the cycle time
  /// requires the device to be in standby mode. Use `set_mode()` to handle
  /// these cases automatically.
  ///
  /// Per the datasheet, the device will take some time to become READY again
  /// after changing the mode:
  ///  * 11ms from cycle -> standby
  ///  * 0.6s for standby -> 3s cycle
  ///  * 2.6s for standby -> 100/300s cycle
  fn set_mode_naive(&mut self, mode: OperationalMode) -> Result<()> {
    match mode {
      OperationalMode::Standby => self.device.smbus_write_byte(0xE5)?,
      OperationalMode::Cycle(period) => {
        // configure the cycle
        self.device.smbus_write_byte_data(0x89, period.to_value())?;

        // per docs, must wait 6ms between commands if commands depend on one
        // another
        self.sleep_write();

        // enter cycle mode
        self.device.smbus_write_byte(0xE4)?;

        // per docs, it takes 11ms to enter cycle mode
        thread::sleep(Duration::from_millis(11));
      }
    }

    trace!("Metriful::set_mode_timeout({:?}): done", mode);

    Ok(())
  }

  /// Changes the device's operational mode. This may block for up to ~3 seconds
  /// if an intermediate mode change is required and/or if the device is not yet
  /// READY to accept commands.
  ///
  /// Per the datasheet, the device will take some time to become READY again
  /// after changing the mode:
  ///  * 11ms from cycle -> standby
  ///  * 0.6s for standby -> 3s cycle
  ///  * 2.6s for standby -> 100/300s cycle
  ///
  /// This function automatically waits the appropriate amount of time for the
  /// device to become ready, then returns an updated DeviceStatus.
  pub fn set_mode_timeout(
    &mut self,
    mode: OperationalMode,
    timeout: Option<Duration>
  ) -> Result<DeviceStatus> {
    use OperationalMode::*;
    self.wait_for_ready_timeout(timeout)?;

    let status = self.read_status()?;
    match (status.mode, mode) {
      // no-op
      (Standby, Standby) => (),
      (Cycle(a), Cycle(b)) if a == b => (),

      // valid
      (Standby, Cycle(_)) => self.set_mode_naive(mode)?,
      (Cycle(_), Standby) => self.set_mode_naive(mode)?,

      // need an intermediate standby
      (Cycle(_), Cycle(_)) => {
        self.set_mode_naive(OperationalMode::Standby)?;
        self.wait_for_ready_timeout(timeout)?;
        self.set_mode_naive(mode)?;
      },
    }

    self.wait_for_ready_timeout(timeout)?;
    trace!("Metriful::set_mode_timeout(): finished, ready");

    Ok(self.read_status()?)
  }

  /// Executes an on-demand measurement.
  ///
  /// Notes:
  ///  * Device must currently be in READY state
  ///  * Device must be in standby mode
  pub fn execute_measurement(&mut self) -> Result<()> {
    let status = match &self.status {
      Some(status) => status,
      None => return Err(MetrifulError::StatusMissing)
    };

    if !matches!(status.mode, OperationalMode::Standby) {
      return Err(MetrifulError::InvalidMode {
        current: status.mode,
        required: OperationalMode::Standby
      });
    }

    self.ensure_ready()?;

    self.device.smbus_write_byte(0xE1)?;
    self.sleep_write();

    trace!("Metriful::execute_measurement(): done");

    Ok(())
  }

  /// Reads the given metric from the device. Note that the device must
  /// currently be in a READY state or an error will be raised.
  pub fn read<U: MetrifulUnit>(&mut self, metric: Metric<U>) -> Result<UnitValue<U>> {
    self.ensure_ready()?;

    let ret = metric.read(&mut self.device);
    trace!("Metriful::read({:x?}) -> {:?}", metric, &ret);
    ret
  }

  pub fn read_iter_timeout<'a, U>(
    &'a mut self,
    metric: Metric<U>,
    interval: Duration,
    timeout: Option<Duration>,
  ) -> MetricReadIterator<U>
  where
    U: MetrifulUnit
  {
    MetricReadIterator {
      device: self,
      error: false,
      last_instant: Instant::now(),
      metric,
      interval,
      timeout,
    }
  }

  /// Returns an iterator that reads the given metric repeatedly at a given
  /// interval. Note that the thread will block for `interval` duration on each
  /// read.
  pub fn read_iter<'a, U>(
    &'a mut self,
    metric: Metric<U>,
    interval: Duration,
  ) -> MetricReadIterator<U>
  where
    U: MetrifulUnit
  {
    MetricReadIterator {
      device: self,
      error: false,
      timeout: None,
      last_instant: Instant::now(),
      metric,
      interval,
    }
  }

  pub fn cycle_read_iter_timeout<'a, U>(
    &'a mut self,
    metric: Metric<U>,
    cycle_period: CyclePeriod,
    timeout: Option<Duration>,
  ) -> CycleReadIterator<U>
  where
    U: MetrifulUnit
  {
    CycleReadIterator {
      device: self,
      first: true,
      error: false,
      metric,
      cycle_period,
      timeout,
    }
  }

  pub fn read_status(&mut self) -> Result<DeviceStatus> {
    let status = DeviceStatus::read(&mut self.device)?;
    self.status = Some(status.clone());
    trace!("Metriful::read_status() -> {:?}", &self.status);

    Ok(status)
  }

  /// Sleeps for 6ms, as recommended after a write.
  pub fn sleep_write(&self) {
    thread::sleep(Duration::from_millis(6));
  }
}
