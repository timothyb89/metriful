use err_derive::Error;
use i2cdev::linux::LinuxI2CError;

#[derive(Debug, Error)]
pub enum MetrifulError {
  #[error(display = "i2c error: {}", _0)]
  I2CError(#[error(source)] LinuxI2CError),

  #[error(display = "gpio error: {}", _0)]
  GPIOError(#[error(source)] sysfs_gpio::Error),

  #[error(display = "invalid particle sensor mode: {:x}", _0)]
  InvalidParticleSensorMode(u8),

  #[error(display = "invalid cycle period mode: {:x}", _0)]
  InvalidCyclePeriod(u8),

  #[error(display = "invalid operational mode: {:x}", _0)]
  InvalidOperationalMode(u8),

  #[error(display = "exceeded timeout waiting for sensor to become ready")]
  ReadyTimeoutExceeded
}

pub type Result<T> = std::result::Result<T, MetrifulError>;
