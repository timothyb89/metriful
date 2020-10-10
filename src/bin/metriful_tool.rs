#[macro_use] extern crate log;

use std::path::PathBuf;
use std::time::Duration;
use std::thread;

use structopt::StructOpt;
use anyhow::{Result, Context};

use sysfs_gpio::{Direction, Pin};
use i2cdev::linux::*;

use metriful::{Metriful, METRIFUL_ADDRESS};
use metriful::metric::*;

fn try_from_hex_arg(s: &str) -> Result<u16> {
  if s.starts_with("0x") {
    u16::from_str_radix(&s[2..], 16).with_context(|| format!("invalid hex: {}", s))
  } else {
    s.parse().with_context(|| format!("invalid int: {}", s))
  }
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "metriful-tool")]
struct Options {
  /// system i2c device, e.g. /dev/i2c-1
  #[structopt(long, short, parse(from_os_str), default_value = "/dev/i2c-1")]
  device: PathBuf,

  /// Metriful device i2c address; usually 0x71, or 0x71 if the solder bridge is
  /// closed. Can specify a plain base-10 int or hex with a `0x` prefix.
  #[structopt(long, parse(try_from_str = try_from_hex_arg), default_value = "0x71")]
  i2c_address: u16,

  /// GPIO number for the ready signal. Note that this is a GPIO number, not a
  /// physical pin number - the mapping between the two numbers varies by
  /// device.
  #[structopt(long, default_value = "11", env = "GPIO_READY")]
  gpio_ready: u64
}

fn main() -> Result<()> {
  let env = env_logger::Env::default()
    .filter_or("METRIFUL_LOG", "info")
    .write_style_or("METRIFUL_STYLE", "always");

  env_logger::Builder::from_env(env)
    .target(env_logger::Target::Stderr)
    .init();

  let opts: Options = Options::from_args();
  debug!("options: {:?}", opts);

  let ready_pin = Pin::new(opts.gpio_ready);
  ready_pin.export()?;
  ready_pin.set_active_low(false)?;
  ready_pin.set_direction(Direction::In)?;

  let device = LinuxI2CDevice::new(opts.device, METRIFUL_ADDRESS)?;

  let mut metriful = Metriful::new(ready_pin, device);

  loop {
    if metriful.is_ready()? {
      break;
    } else {
      warn!("sensor is not ready, waiting... ({:?})", ready_pin.get_value());
      thread::sleep(Duration::from_millis(100));
    }
  }

  info!("metriful sensor is ready");

  loop {
    println!("temperature: {}", metriful.read(&METRIC_TEMPERATURE)?);
    println!("pressure:    {}", metriful.read(&METRIC_PRESSURE)?);
    thread::sleep(Duration::from_millis(5000));
  }
}