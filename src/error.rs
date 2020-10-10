use err_derive::Error;
use i2cdev::linux::LinuxI2CError;

#[derive(Debug, Error)]
pub enum MetrifulError {
  #[error(display = "i2c error: {}", _0)]
  I2CError(#[error(source)] LinuxI2CError),

  #[error(display = "gpio error: {}", _0)]
  GPIOError(#[error(source)] sysfs_gpio::Error)
}

pub type Result<T> = std::result::Result<T, MetrifulError>;
