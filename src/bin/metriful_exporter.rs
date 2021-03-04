use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use color_eyre::eyre::{Result, Context};
use log::*;
use metriful::unit::{MetrifulUnit, UnitCombinedData};
use metriful::{Metriful, CyclePeriod, metric::METRIC_COMBINED_ALL, unit::UnitValue};
use serde::Serialize;
use serde_json::{self, json};
use simple_prometheus_exporter::{Exporter, export};
use structopt::StructOpt;
use tokio::task;
use tokio_stream::{self as stream, StreamExt};
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

#[derive(Debug, Clone, StructOpt, Serialize)]
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

type Reading = Option<UnitValue<UnitCombinedData>>;

fn export_reading(
  exporter: &Exporter,
  reading: &Reading,
  read_count: &Arc<AtomicUsize>,
  error_count: &Arc<AtomicUsize>,
) -> String {
  let mut s = exporter.session();

  match reading {
    Some(r) => {
      export!(s, "metriful_ready", 1);

      let air = &r.value.air.value;
      export!(
        s, "metriful_air_gas_sensor_resistance", air.gas_sensor_resistance.value,
        unit = air.gas_sensor_resistance.unit.get_name()
      );
      export!(
        s, "metriful_air_humidity", air.humidity.value,
        unit = air.humidity.unit.get_name()
      );
      export!(
        s, "metriful_air_pressure", air.pressure.value,
        unit = air.pressure.unit.get_name()
      );
      export!(
        s, "metriful_air_temperature", air.temperature.value,
        unit = air.temperature.unit.get_name()
      );

      let air_quality = &r.value.air_quality.value;
      export!(
        s, "metriful_air_quality_aqi", air_quality.aqi.value,
        unit = air_quality.aqi.unit.get_name()
      );
      export!(
        s, "metriful_air_quality_aqi_accuracy", air_quality.aqi_accuracy.value.to_uint(),
        unit = air_quality.aqi_accuracy.unit.get_name()
      );
      export!(
        s, "metriful_air_quality_estimated_co2", air_quality.estimated_co2.value,
        unit = air_quality.estimated_co2.unit.get_name()
      );
      export!(
        s, "metriful_air_quality_estimated_voc", air_quality.estimated_voc.value,
        unit = air_quality.estimated_voc.unit.get_name()
      );

      let light = &r.value.light.value;
      export!(
        s, "metriful_light_illuminance", light.illuminance.value,
        unit = light.illuminance.unit.get_name()
      );
      export!(
        s, "metriful_light_white_level", light.white_level.value,
        unit = light.white_level.unit.get_name()
      );

      // TODO: particle sensors not currently supported as we'd need to
      // configure the sensor at startup
      // TODO: we could _technically_ still expose the values here if users
      // somehow configured it, since we know the initial_status, but this seems
      // unlikely.
      // let particle = &r.value.particle.value;
      // export!(
      //   s, "metriful_particle_concentration", particle.concentration.value,
      //   unit = particle.concentration.unit.get_name()
      // );
      // TODO: duty_cycle, validity

      let sound = &r.value.sound.value;
      export!(
        s, "metriful_sound_measurement_stable",
        sound.measurement_stability.value.to_uint(),
        unit = sound.measurement_stability.unit.get_name()
      );
      export!(
        s, "metriful_sound_peak_amplitude",
        sound.peak_amplitude.value,
        unit = sound.peak_amplitude.unit.get_name()
      );
      export!(
        s, "metriful_sound_weighted_spl",
        sound.weighted_spl.value,
        unit = sound.weighted_spl.unit.get_name()
      );

      let [b1, b2, b3, b4, b5, b6] = sound.spl_bands.value.0;
      export!(
        s, "metriful_sound_spl_band",
        b1,
        unit = "decibels",
        band = "1",
        band_midpoint_hz = "125",
        band_lower_hz = "88",
        band_upper_hz = "177"
      );
      export!(
        s, "metriful_sound_spl_band",
        b2,
        unit = "decibels",
        band = "2",
        band_midpoint_hz = "250",
        band_lower_hz = "177",
        band_upper_hz = "354"
      );
      export!(
        s, "metriful_sound_spl_band",
        b3,
        unit = "decibels",
        band = "3",
        band_midpoint_hz = "500",
        band_lower_hz = "354",
        band_upper_hz = "707"
      );
      export!(
        s, "metriful_sound_spl_band",
        b4,
        unit = "decibels",
        band = "4",
        band_midpoint_hz = "1000",
        band_lower_hz = "707",
        band_upper_hz = "1414"
      );
      export!(
        s, "metriful_sound_spl_band",
        b5,
        unit = "decibels",
        band = "5",
        band_midpoint_hz = "2000",
        band_lower_hz = "1414",
        band_upper_hz = "2828"
      );
      export!(
        s, "metriful_sound_spl_band",
        b6,
        unit = "decibels",
        band = "6",
        band_midpoint_hz = "4000",
        band_lower_hz = "2828",
        band_upper_hz = "5657"
      );
    },
    None => {
      export!(s, "metriful_ready", 0);
    }
  };

  export!(s, "metriful_read_count", read_count.load(Ordering::Relaxed) as f64);
  export!(s, "metriful_error_count", error_count.load(Ordering::Relaxed) as f64);

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
  let read_count = Arc::new(AtomicUsize::new(0));
  let error_count = Arc::new(AtomicUsize::new(0));

  // initialize the sensor and start the async read thread
  let sensor_opts = opts.clone();
  let res: Result<_> = task::spawn_blocking(move || {
    let mut metriful = Metriful::try_new(
      sensor_opts.gpio_ready,
      sensor_opts.device,
      sensor_opts.i2c_address
    ).wrap_err("could not initialize sensor")?;

    metriful.wait_for_ready_timeout(sensor_opts.timeout)
      .wrap_err("sensor did not become ready in time")?;

    metriful.reset().wrap_err("sensor reset failed")?;

    // fetch the initial status while we're here - we need it to determine the
    // particle sensor type, if any
    let status = metriful.read_status()
      .wrap_err("could not read sensor status")?;

    info!("sensor is ready, status: {:?}", &status);

    let handles = metriful.async_cycle_read_timeout(
      *METRIC_COMBINED_ALL,
      sensor_opts.interval,
      sensor_opts.timeout
    );

    Ok((status, handles))
  }).await?;

  // unpack the channel + handle (separate for type inference reasons)
  let (initial_status, (_tx, rx, _handle)) = res?;

  // spawn a task to continuously move the latest reading into latest_reading_lock
  let data_lock = Arc::clone(&latest_reading_lock);
  let data_read_count = Arc::clone(&read_count);
  let data_error_count = Arc::clone(&error_count);
  task::spawn_blocking(move || {
    for reading in rx.iter() {
      match reading {
        Ok(reading) => match data_lock.try_write() {
          Ok(mut r) => {
            *r = Some(reading);
            data_read_count.fetch_add(1, Ordering::Relaxed);
          },
          Err(e) => {
            error!("could not acquire write lock, reading will be dropped: {}", e);
            data_error_count.fetch_add(1, Ordering::Relaxed);
          }
        },
        Err(e) => {
          error!("error in sensor read: {}", e);
          data_error_count.fetch_add(1, Ordering::Relaxed);
        }
      }
    }
  });

  // json endpoint
  let json_lock = Arc::clone(&latest_reading_lock);
  let json_read_count = Arc::clone(&read_count);
  let json_error_count = Arc::clone(&error_count);
  let json_opts = opts.clone();
  let r_json = warp::path("json").map(move || {
    trace!("exporter: /json");
    match *json_lock.read().unwrap() {
      Some(ref r) => warp::reply::json(&json!({
        "initial_status": &initial_status,
        "reading": r,
        "options": json_opts,
        "error_count": json_error_count.load(Ordering::Relaxed),
        "read_count": json_read_count.load(Ordering::Relaxed),
      })),
      None => warp::reply::json(&json!(null))
    }
  });

  let exporter = Arc::new(Exporter::new());
  let metrics_lock = Arc::clone(&latest_reading_lock);
  let metrics_read_count = Arc::clone(&read_count);
  let metrics_error_count = Arc::clone(&error_count);
  let r_metrics = warp::path("metrics").map(move || {
    trace!("exporter: /metrics");
    export_reading(
      &exporter,
      &*metrics_lock.read().unwrap(),
      &metrics_read_count,
      &metrics_error_count,
    )
  });

  info!("starting exporter on port {}", port);

  let routes = warp::get().and(r_json).or(r_metrics);
  warp::serve(routes).run(([0, 0, 0, 0], port)).await;

  Ok(())
}
