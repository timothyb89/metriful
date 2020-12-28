use std::path::PathBuf;
use std::str::FromStr;
use std::time::{Duration, Instant};
use std::thread;

use color_eyre::eyre::{Result, Error, Context, eyre};
use log::*;
use structopt::StructOpt;

use metriful::{CyclePeriod, Metriful, OperationalMode};
use metriful::metric::*;

fn try_from_hex_arg(s: &str) -> Result<u16> {
  if s.starts_with("0x") {
    u16::from_str_radix(&s[2..], 16).with_context(|| format!("invalid hex: {}", s))
  } else {
    s.parse().with_context(|| format!("invalid int: {}", s))
  }
}

fn try_watch_interval_from_str(s: &str) -> Result<Duration> {
  let seconds: u64 = s.strip_suffix("s")
    .unwrap_or(s)
    .parse()
    .with_context(|| format!("invalid duration in seconds: {:?}", s))?;

  if seconds == 0 {
    return Err(eyre!("interval must be at least 1 second"));
  }

  Ok(Duration::from_secs(seconds))
}

#[derive(Debug, Copy, Clone)]
enum OutputMode {
  Plain,
  JSON,
  CSV
}

impl FromStr for OutputMode {
  type Err = Error;
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    match s.to_ascii_lowercase().as_str() {
      "plain" => Ok(OutputMode::Plain),
      "json" => Ok(OutputMode::JSON),
      "csv" => Ok(OutputMode::CSV),
      s => Err(eyre!("invalid output mode '{}', expected one of: plain, json, csv", s))
    }
  }
}

#[derive(Debug, Clone, StructOpt)]
struct InfoAction {
  /// Data output format, one of: plain, json, csv
  #[structopt(long, short, default_value = "plain")]
  output: OutputMode,
}

#[derive(Debug, Clone, StructOpt)]
struct WatchAction {
  /// If set, writes incoming queries to stdout in the given format. Note that
  /// log messages are always written to stderr. JSON messages are one JSON
  /// object per line. One of: plain, json, csv
  #[structopt(long, short, default_value = "plain")]
  output: OutputMode,

  /// Time interval between measurements in seconds
  #[structopt(
    long, short,
    default_value = "2",
    parse(try_from_str = try_watch_interval_from_str)
  )]
  interval: Duration,
}

#[derive(Debug, Clone, StructOpt)]
struct CycleWatchAction {
  /// Data output format, one of: plain, json, csv
  #[structopt(long, short, default_value = "plain")]
  output: OutputMode,

  /// Cycle period, one of: 0 (3s), 1 (100s), 2 (300s)
  #[structopt(long, short, default_value = "3s")]
  interval: CyclePeriod
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(rename_all = "kebab-case")]
enum Action {
  /// Fetches sensor information
  Info(InfoAction),

  /// Resets the sensor
  Reset,

  /// Displays sensor events
  Watch(WatchAction),

  /// Displays sensor events in cycle mode
  CycleWatch(CycleWatchAction),

  /// Displays sensor events in async cycle mode. This is meant as a library
  /// example and is not functionally different from regular `cycle-watch`.
  CycleWatchAsync(CycleWatchAction),
}

fn parse_duration_secs(s: &str) -> Result<Duration> {
  Ok(Duration::from_secs(
    s.parse().wrap_err_with(|| format!("invalid seconds value: {}", s))?
  ))
}

#[derive(Debug, Clone, StructOpt)]
#[structopt(name = "metriful-tool")]
struct Options {
  /// system i2c device, e.g. /dev/i2c-1
  #[structopt(long, short, parse(from_os_str), default_value = "/dev/i2c-1", global = true)]
  device: PathBuf,

  /// Metriful device i2c address; usually 0x71, or 0x71 if the solder bridge is
  /// closed. Can specify a plain base-10 int or hex with a `0x` prefix.
  #[structopt(long, parse(try_from_str = try_from_hex_arg), default_value = "0x71", global = true)]
  i2c_address: u16,

  /// GPIO number for the ready signal. Note that this is a GPIO number, not a
  /// physical pin number - the mapping between the two numbers varies by
  /// device.
  #[structopt(long, default_value = "11", env = "GPIO_READY", global = true)]
  gpio_ready: u64,

  /// Global timeout for any individual sensor command in seconds.
  #[structopt(long, parse(try_from_str = parse_duration_secs), global = true)]
  timeout: Option<Duration>,

  #[structopt(subcommand)]
  action: Action
}

fn show_info(_opts: &Options, action: &InfoAction, mut metriful: Metriful) -> Result<()> {
  let status = metriful.read_status()?;

  match action.output {
    OutputMode::Plain => println!("{:#?}", status),
    OutputMode::JSON => println!("{}", serde_json::to_string(&status)?),
    _ => return Err(eyre!("csv info not implemented")),
  }

  Ok(())
}

fn reset(_opts: &Options, mut metriful: Metriful) -> Result<()> {
  metriful.reset()?;
  info!("reset command sent, waiting for ready...");

  let now = Instant::now();
  metriful.wait_for_ready()?;

  info!("reset finished, device became ready in {:?}", now.elapsed());

  Ok(())
}

fn watch(opts: &Options, action: &WatchAction, mut metriful: Metriful) -> Result<()> {
  metriful.set_mode_timeout(OperationalMode::Standby, opts.timeout)?;

  loop {
    metriful.execute_measurement()?;
    metriful.wait_for_ready()?;

    let result = metriful.read(*METRIC_COMBINED_ALL)?;

    match action.output {
      OutputMode::Plain => {
        println!(
          "air data:\n{}",
          textwrap::indent(&result.value.air.to_string(), "  ")
        );

        println!(
          "light data:\n{}",
          textwrap::indent(&result.value.light.to_string(), "  ")
        );

        println!(
          "sound data:\n{}",
          textwrap::indent(&result.value.sound.to_string(), "  ")
        );

        println!(
          "particle data: \n{}",
          textwrap::indent(&result.value.particle.to_string(), "  ")
        );

        println!("---");
      },
      OutputMode::JSON => println!("{}", serde_json::to_string(&result)?),
      OutputMode::CSV => return Err(eyre!("csv output not implemented")),
    }

    thread::sleep(action.interval);
  }
}

fn cycle_watch(opts: &Options, action: &CycleWatchAction, mut metriful: Metriful) -> Result<()> {
  let iter = metriful.cycle_read_iter_timeout(
    *METRIC_COMBINED_ALL,
    action.interval,
    opts.timeout
  );
  for value in iter {
    let value = value?;

    match &action.output {
      OutputMode::Plain => {
        println!("{}", value);
        println!("---");
      },
      OutputMode::JSON => {
        println!("{}", serde_json::to_string(&value)?)
      }
      OutputMode::CSV => return Err(eyre!("csv output not implemented")),
    }
  }

  Ok(())
}

fn cycle_watch_async(opts: &Options, action: &CycleWatchAction, metriful: Metriful) -> Result<()> {
  let (_cmd_tx, metric_rx, _handle) = metriful.async_cycle_read_timeout(
    *METRIC_COMBINED_ALL,
    action.interval,
    opts.timeout
  );

  loop {
    if let Ok(value) = metric_rx.try_recv() {
      println!();

      let value = value?;

      match &action.output {
        OutputMode::Plain => {
          println!("{}", value);
          println!("---");
        },
        OutputMode::JSON => {
          println!("{}", serde_json::to_string(&value)?)
        }
        OutputMode::CSV => return Err(eyre!("csv output not implemented")),
      }
    }

    thread::sleep(Duration::from_millis(100));
  }
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

  let metriful = Metriful::try_new(opts.gpio_ready, &opts.device, opts.i2c_address)?;
  info!("waiting for sensor to become ready...");
  metriful.wait_for_ready()?;

  info!("metriful sensor is ready");

  match &opts.action {
    Action::Info(action) => show_info(&opts, &action, metriful)?,
    Action::Reset => reset(&opts, metriful)?,
    Action::Watch(action) => watch(&opts, &action, metriful)?,
    Action::CycleWatch(action) => cycle_watch(&opts, &action, metriful)?,
    Action::CycleWatchAsync(action) => cycle_watch_async(&opts, &action, metriful)?,
  };

  Ok(())
}
