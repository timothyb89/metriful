use std::{time::Instant, path::PathBuf};
use std::time::Duration;
use std::thread;

use color_eyre::eyre::{Result, Context};
use log::*;
use structopt::StructOpt;

use metriful::Metriful;
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
  color_eyre::install()?;

  let env = env_logger::Env::default()
    .filter_or("METRIFUL_LOG", "info")
    .write_style_or("METRIFUL_STYLE", "always");

  env_logger::Builder::from_env(env)
    .target(env_logger::Target::Stderr)
    .init();

  let opts: Options = Options::from_args();
  debug!("options: {:?}", opts);

  let mut metriful = Metriful::try_new(opts.gpio_ready, &opts.device, opts.i2c_address)?;
  info!("waiting for sensor to become ready...");
  metriful.wait_for_ready()?;

  info!("metriful sensor is ready");

  info!("device status: {:#?}", metriful.read_status());

  let instant = Instant::now();
  let temps = metriful.read_iter(*METRIC_TEMPERATURE, Duration::from_secs(3))
    .take(5)
    .collect::<Vec<_>>();
  info!("read_iter took {:?} and collected: {:?}", instant.elapsed(), &temps);

  loop {
    metriful.execute_measurement()?;
    metriful.wait_for_ready()?;

    println!("temperature: {}", metriful.read(*METRIC_TEMPERATURE)?);
    println!("pressure:    {}", metriful.read(*METRIC_PRESSURE)?);
    println!("illuminance: {}", metriful.read(*METRIC_ILLUMINANCE)?);
    println!("white level: {}", metriful.read(*METRIC_WHITE_LIGHT_LEVEL)?);
    thread::sleep(Duration::from_millis(1000));
  }
}
