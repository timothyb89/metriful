use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use std::sync::mpsc::channel;

use color_eyre::eyre::{Result, Error, Context, eyre};
use log::*;
use metriful::{Metriful, CyclePeriod, metric::METRIC_COMBINED_ALL};
use serde_json::{self, json};
use simple_prometheus_exporter::{Exporter, export};
use structopt::StructOpt;
use warp::Filter;

fn try_from_hex_arg(s: &str) -> Result<u16> {
  if s.starts_with("0x") {
    u16::from_str_radix(&s[2..], 16).with_context(|| format!("invalid hex: {}", s))
  } else {
    s.parse().with_context(|| format!("invalid int: {}", s))
  }
}

fn parse_duration_secs(s: &str) -> Result<Duration> {
  Ok(Duration::from_secs(
    s.parse().wrap_err_with(|| format!("invalid seconds value: {}", s))?
  ))
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "metriful-exporter")]
struct Options {
  /// system i2c device, e.g. /dev/i2c-1
  #[structopt(
    long, short,
    parse(from_os_str),
    default_value = "/dev/i2c-1",
    global = true,
    env = "METRIFUL_I2C_DEVICE"
  )]
  device: PathBuf,

  /// Metriful device i2c address; usually 0x71, or 0x71 if the solder bridge is
  /// closed. Can specify a plain base-10 int or hex with a `0x` prefix.
  #[structopt(
    long,
    parse(try_from_str = try_from_hex_arg),
    default_value = "0x71",
    global = true,
    env = "METRIFUL_I2C_ADDRESS"
  )]
  i2c_address: u16,

  /// GPIO number for the ready signal. Note that this is a GPIO number, not a
  /// physical pin number - the mapping between the two numbers varies by
  /// device.
  #[structopt(
    long,
    default_value = "11",
    env = "METRIFUL_GPIO_READY",
    global = true
  )]
  gpio_ready: u64,

  /// Global timeout for any individual sensor command in seconds.
  #[structopt(
    long,
    parse(try_from_str = parse_duration_secs),
    global = true,
    env = "METRIFUL_TIMEOUT"
  )]
  timeout: Option<Duration>,

  /// Cycle period, one of: 0 (3s), 1 (100s), 2 (300s)
  #[structopt(long, short, default_value = "3s", env = "METRIFUL_INTERVAL")]
  interval: CyclePeriod,

  /// HTTP server port
  #[structopt(long, short, default_value = "8083", env = "METRIFUL_PORT")]
  port: u16,
}

fn export_reading(
  exporter: &Exporter,
  reading: &Reading,
  error_count: &Arc<AtomicUsize>,
  fatal_error_count: &Arc<AtomicUsize>
) -> String {
  let mut s = exporter.session();

  match reading {
    Some(r) => {
      // TODO
      //export!(s, "metriful_temperature", r.temperature.value, unit = "c");
    },
    None => ()
  };

  export!(s, "metriful_error_count", error_count.load(Ordering::Relaxed) as f64);
  export!(s, "metriful_fatal_error_count", fatal_error_count.load(Ordering::Relaxed) as f64);

  s.to_string()
}

#[tokio::main]
async fn main() -> Result<()> {
  color_eyre::install()?;

  let env = env_logger::Env::default()
    .filter_or("METRIFUL_LOG", "info")
    .write_style_or("METRIFUL_STYLE", "always");

  env_logger::Builder::from_env(env)
    .target(env_logger::Target::Stderr)
    .init();

  let opts = Options::from_args();
  let port = opts.port;

  let latest_reading_lock = Arc::new(RwLock::new(None));
  let error_count = Arc::new(AtomicUsize::new(0));
  let fatal_error_count = Arc::new(AtomicUsize::new(0));

  let json_lock = Arc::clone(&latest_reading_lock);
  let r_json = warp::path("json").map(move || {
    match *json_lock.read().unwrap() {
      Some(ref r) => warp::reply::json()),
      None => warp::reply::json(&json!(null))
    }
  });

  let exporter = Arc::new(Exporter::new());
  let metrics_lock = Arc::clone(&latest_reading_lock);
  let metrics_error_count = Arc::clone(&error_count);
  let metrics_fatal_error_count = Arc::clone(&fatal_error_count);
  let r_metrics = warp::path("metrics").map(move || {
    export_reading(
      &exporter,
      &*metrics_lock.read().unwrap(),
      &metrics_error_count,
      &metrics_fatal_error_count
    )
  });

  info!("starting exporter on port {}", port);

  let routes = warp::get().and(r_json).or(r_metrics);
  warp::serve(routes).run(([0, 0, 0, 0], port)).await;

  Ok(())
}
