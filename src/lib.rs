//! A library for configuring and reading from Metriful MS430 indoor
//! environment sensors.
//!
//! This library targets Raspberry Pis and other Linux-based hosts as supported
//! by [`i2cdev`] and [`sysfs_gpio`].
//!
//! ### Getting Started
//! 
//! The library is designed to be straightforward to use:
//!  1. Connect the device per [Metriful's docs](https://github.com/metriful/sensor#raspberry-pi)
//!  2. Open the device using [`Metriful::try_new_timeout()`]
//!  3. Read metrics using one of the various helper functions:
//!     * [`Metriful::read_iter_timeout()`]: reads continuously at a
//!       user-defined interval
//!     * [`Metriful::cycle_read_iter_timeout()`]: reads continuously at a set
//!       interval with the device in cycle mode
//!     * [`Metriful::async_cycle_read_timeout()`]: reads continuously in a
//!       background thread and reports results via a
//!       [`std::sync::mpsc::channel`]
//!     * [`Metriful::read()`]: to read a single metric once
//!
//! The various read functions need to be told which metric to read; see the
//! [`metric`] module for a complete list of possibilities. To read more than
//! one metric at once, a number of "combined read" pseudo-metrics are
//! provided:
//!  * [`struct@METRIC_COMBINED_AIR_DATA`]: all air data
//!  * [`struct@METRIC_COMBINED_AIR_QUALITY_DATA`]: all air quality data; only valid
//!    in cycle mode
//!  * [`struct@METRIC_COMBINED_LIGHT_DATA`]: all light data
//!  * [`struct@METRIC_COMBINED_SOUND_DATA`]: all sound data
//!  * [`struct@METRIC_COMBINED_PARTICLE_DATA`]: all particle data; only valid if an
//!    external particulate sensor is attached
//!  * [`struct@METRIC_COMBINED_ALL`]: all data; air quality data is only valid in
//!    cycle mode
//!
//! ### Example
//! To open the device and continuously read all available metrics:
//! 
//! ```no_run
//! use std::time::Duration;
//! use metriful::{Metriful, CyclePeriod, metric::*};
//!
//! # fn main() -> metriful::error::Result<()> {
//! let mut metriful = Metriful::try_new(17, "/dev/i2c-1", 0x71)?;
//!
//! let iter = metriful.cycle_read_iter_timeout(
//!   *METRIC_COMBINED_ALL,
//!   CyclePeriod::Period0,
//!   Some(Duration::from_secs(3))
//! );
//! for metric in iter {
//!   let metric = metric?;
//!   println!("{}", metric);
//! }
//! # Ok(())
//! # }
//! ```

use std::fmt;
use std::path::Path;
use std::time::{Duration, Instant};
use std::sync::mpsc::{channel, Sender, Receiver};
use std::thread::{self, JoinHandle};

use i2cdev::core::*;
use i2cdev::linux::LinuxI2CDevice;
use log::trace;
use sysfs_gpio::{Direction, Pin};

pub mod error;
pub mod metric;
pub mod status;
pub mod unit;
pub mod util;

use error::*;
use metric::*;
pub use status::*;
use unit::*;

/// Metriful i2c address. Note: 0x70 if solder bridge is closed.
pub const METRIFUL_ADDRESS: u16 = 0x71;

pub const READY_POLL_INTERVAL: u64 = 10;

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

/// A Metriful MS430 sensor connected via I2C with a "ready" GPIO pin.
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

impl Metriful {
  /// Creates a new Metriful given a preexisting GPIO [`Pin`] and
  /// [`LinuxI2CDevice`]. This ensures the device is ready and fetches the
  /// current state. Returns an error if the timeout is set and exceeded, or if
  /// device status cannot be read.
  ///
  /// Note that this does not reset the device. The manual recommends doing so
  /// before use; call [`Metriful::reset()`] to do so.
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
  /// before use; call [`Metriful::reset()`] to do so.
  pub fn try_new_timeout(
    gpio_ready: u64,
    i2c_device: impl AsRef<Path>,
    i2c_address: u16,
    timeout: Option<Duration>
  ) -> Result<Metriful> {
    trace!(
      "Metriful::try_new_timeout({}, {}, {:x}, {:?})",
      gpio_ready, i2c_device.as_ref().display(), i2c_address, timeout
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
  /// before use; call [`Metriful::reset()`] to do so.
  pub fn try_new(
    gpio_ready: u64,
    i2c_device: impl AsRef<Path>,
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

  /// Sleeps the thread until [`Metriful::is_ready()`] returns true, polling every
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

  /// Sleeps the thread until [`Metriful::is_ready()`] returns true, polling
  /// every 10ms. This has no timeout and will wait indefinitely; see
  /// [`Metriful::wait_for_ready_timeout()`] if a timeout is desired.
  pub fn wait_for_ready(&self) -> Result<()> {
    self.wait_for_ready_timeout(None)
  }

  /// The inverse of [`Metriful::wait_for_ready_timeout()`], this waits until
  /// the device is explicitly **not** ready, useful for e.g. waiting for a new
  /// cycle period.
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

  /// Waits for [`Metriful::is_ready()`] to become true and executes the given
  /// function. This has no timeout and may wait indefinitely.
  pub fn execute_when_ready<T>(
    &mut self,
    func: impl FnOnce(&mut Metriful) -> T,
  ) -> Result<T> {
    self.execute_when_ready_timeout(func, None)
  }

  /// Sends a device reset command, waits for it to become ready again, and
  /// returns a refreshed [`DeviceStatus`]. Raises an error if the device i
  /// not initially ready.
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
  /// requires the device to be in standby mode. Use [`Metriful::set_mode()`]
  /// to handle these cases automatically.
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
  ///
  /// # Example
  /// ```no_run
  /// use metriful::{Metriful, metric::*};
  ///
  /// # fn main() -> metriful::error::Result<()> {
  /// let mut metriful = Metriful::try_new(17, "/dev/i2c-1", 0x71)?;
  ///
  /// println!("{}", metriful.read(*METRIC_COMBINED_ALL)?);
  /// # Ok(())
  /// # }
  /// ```
  pub fn read<U: MetrifulUnit>(&mut self, metric: Metric<U>) -> Result<UnitValue<U>> {
    self.ensure_ready()?;

    let ret = metric.read(&mut self.device);
    trace!("Metriful::read({:x?}) -> {:?}", metric, &ret);
    ret
  }

  /// Returns an iterator that reads the given metric repeatedly at a given
  /// interval. Note that the thread will block for `interval` duration on each
  /// read. It reads indefinitely or until an error occurs.
  ///
  /// Note that this iterator performs "on-demand" measurements and as such
  /// certain metrics will not be available, particularly air quality data.
  /// Consider using [`Metriful::cycle_read_iter_timeout()`] for these values.
  ///
  /// Only a single "metric" may be read per iteration, however various
  /// combined pseudo-metrics can be be used to read more data, including
  /// [`struct@METRIC_COMBINED_ALL`].
  ///
  /// See the [`MetricReadIterator`] documentation for further information.
  ///
  /// # Example
  /// ```no_run
  /// use std::time::Duration;
  /// use metriful::{Metriful, metric::*};
  ///
  /// # fn main() -> metriful::error::Result<()> {
  /// let mut metriful = Metriful::try_new(17, "/dev/i2c-1", 0x71)?;
  ///
  /// let iter = metriful.read_iter_timeout(
  ///   *METRIC_COMBINED_ALL,
  ///   Duration::from_secs(3),
  ///   Some(Duration::from_secs(3))
  /// );
  /// for metric in iter {
  ///   let metric = metric?;
  ///   println!("{}", metric);
  /// }
  /// # Ok(())
  /// # }
  /// ```
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
  /// read. It reads indefinitely or until an error occurs.
  ///
  /// Note that this iterator performs "on-demand" measurements and as such
  /// certain metrics will not be available, particularly air quality data.
  /// Consider using [`Metriful::cycle_read_iter_timeout()`] for these values.
  ///
  /// Only a single "metric" may be read per iteration, however various
  /// combined pseudo-metrics can be be used to read more data, including
  /// [`struct@METRIC_COMBINED_ALL`].
  ///
  /// This may block indefinitely if device communication fails; consider using
  /// [`Metriful::read_iter_timeout()`] to specify a timeout.
  ///
  /// See the [`MetricReadIterator`] documentation for further information.
  ///
  /// # Example
  /// ```no_run
  /// use std::time::Duration;
  /// use metriful::{Metriful, metric::*};
  ///
  /// # fn main() -> metriful::error::Result<()> {
  /// let mut metriful = Metriful::try_new(17, "/dev/i2c-1", 0x71)?;
  ///
  /// for metric in metriful.read_iter(*METRIC_COMBINED_ALL, Duration::from_secs(3)) {
  ///   let metric = metric?;
  ///   println!("{}", metric);
  /// }
  /// # Ok(())
  /// # }
  /// ```
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

  /// Returns an iterator that reads the given metric repeatedly at the given
  /// device-supported [`CyclePeriod`]. Note that the thread will block for
  /// `interval` duration on each read. It reads indefinitely or until an error
  /// occurs.
  ///
  /// Only a single "metric" may be read per iteration, however various
  /// combined pseudo-metrics can be be used to read more data, including
  /// [`struct@METRIC_COMBINED_ALL`].
  ///
  /// See the [`CycleReadIterator`] documentation for further information.
  ///
  /// # Example
  /// ```no_run
  /// use std::time::Duration;
  /// use metriful::{Metriful, CyclePeriod, metric::*};
  ///
  /// # fn main() -> metriful::error::Result<()> {
  /// let mut metriful = Metriful::try_new(17, "/dev/i2c-1", 0x71)?;
  ///
  /// let iter = metriful.cycle_read_iter_timeout(
  ///   *METRIC_COMBINED_ALL,
  ///   CyclePeriod::Period0,
  ///   Some(Duration::from_secs(3)),
  /// );
  ///
  /// for metric in iter {
  ///   let metric = metric?;
  ///   println!("{}", metric);
  /// }
  /// # Ok(())
  /// # }
  /// ```
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

  /// Spawns an async cycle read thread that reports metrics.
  ///
  /// This function returns three objects callers may interact with:
  ///  * `cmd_tx`: send the unit value `()` via this channel to ask the
  ///    background thread to terminate, e.g. `cmd_tx.send(())?`
  ///  * `metric_rx`: read metrics are periodically sent here
  ///  * `handle`: a thread JoinHandle
  ///
  /// This takes ownership of the `Metriful` instance for as long as the
  /// background thread is alive. The original owned [`Metriful`] is returned
  /// via `.join()` on the returned `JoinHandle`. Send the unit value `()` via
  /// `cmd_tx` (e.g. `cmd_tx.send(())?`) to ask the thread to terminate before
  /// attempting to join it to avoid a deadlock.
  ///
  /// If an error occurs, it will be sent via `metric_rx` and the thread will
  /// terminate.
  pub fn async_cycle_read_timeout<U>(
    mut self,
    metric: Metric<U>,
    cycle_period: CyclePeriod,
    timeout: Option<Duration>,
  ) -> (Sender<()>, Receiver<Result<UnitValue<U>>>, JoinHandle<Metriful>)
  where
    U: MetrifulUnit + 'static
  {
    let (cmd_tx, cmd_rx) = channel();
    let (metric_tx, metric_rx) = channel();

    let handle = thread::spawn(move || {
      let iter = self.cycle_read_iter_timeout(metric, cycle_period, timeout);

      for metric in iter {
        if cmd_rx.try_recv().is_ok() {
          trace!("Metriful::async_cycle_read_timeout(): break");
          break;
        }

        let metric = match metric {
          Ok(m) => m,
          Err(e) => {
            metric_tx.send(Err(e)).ok();
            break;
          }
        };

        match metric_tx.send(Ok(metric)) {
          Ok(_) => (),
          Err(_e) => {
            // channel is dead, just quit
            break;
          }
        }
      }

      self
    });

    (cmd_tx, metric_rx, handle)
  }

  /// Fetches the current device status. This does *not* wait for the device to
  /// become ready and may fail if [`Metriful::is_ready()`] is false.
  ///
  /// # Example
  /// ```no_run
  /// use metriful::Metriful;
  ///
  /// # fn main() -> metriful::error::Result<()> {
  /// let mut metriful = Metriful::try_new(17, "/dev/i2c-1", 0x71)?;
  ///
  /// println!("{:#?}", metriful.read_status()?);
  /// # Ok(())
  /// # }
  /// ```
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
