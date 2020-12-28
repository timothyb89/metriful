# `metriful-exporter`

A Prometheus exporter and Rust crate for the [Metriful][metriful] sensor.

As it uses [`rust-i2cdev`] and [`rust-sysfs-gpio`], it needs to run on a Linux
host that supports I2C and GPIO, such as the Raspberry Pi.

Requires rustc >= 1.48.

[metriful]: https://github.com/metriful/sensor
[`rust-i2cdev`]: https://github.com/rust-embedded/rust-i2cdev
[`rust-sysfs-gpio`]: https://github.com/rust-embedded/rust-sysfs-gpio

## `metriful-tool`

`metriful-tool` can be used to query and manage Metriful sensors.

### Viewing sensor configuration: `metriful-tool info`

```
pi@airq:~ $ ./metriful-tool info
[2020-12-12T02:03:39Z INFO  metriful_tool] waiting for sensor to become ready...
[2020-12-12T02:03:39Z INFO  metriful_tool] metriful sensor is ready
DeviceStatus {
    particle_sensor: Disabled,
    light_int: Disabled,
    sound_int: Disabled,
    mode: Cycle(
        CyclePeriod(
            "3s",
        ),
    ),
}
```

This subcommand supports JSON output with `metriful-tool info -o json`

### Watching metrics: `metriful-tool watch`

Reads metrics at a user-configurable interval. Note that this performs
"on-demand" measurements and as such will not include valid air quality data;
use `cycle-watch` to get this data.

```
pi@airq:~ $ ./metriful-tool watch
[2020-12-12T02:12:22Z INFO  metriful_tool] waiting for sensor to become ready...
[2020-12-12T02:12:22Z INFO  metriful_tool] metriful sensor is ready
air data:
  temperature:           17.9 ℃
  pressure:              84958 Pa
  humidity:              20.9 % RH
  gas sensor resistance: 80513 Ω

light data:
  illuminance: 10.9 lx
  white level: 160

sound data:
  a-weighted SPL:        61.7 dBa
  SPL frequency bands:   [43.2, 36.3, 41.6, 54.5, 58.0, 53.6]
  peak amplitude:        9263.1 mPa
  measurement stability: unstable

particle data:
  duty cycle:    0 %
  concentration: 0
  validity:      initializing

---

[...]
```

The default interval (2s) can be overridden with `-i <seconds>`. Note that
intervals below 2s may report inaccurate measurements.

This subcommand supports JSON output with `metriful-tool watch -o json`; JSON
documents are separated by newlines to stdout and can be consumed by e.g. `jq`.

### Watching metrics: `metriful-tool cycle-watch`

Reads metrics at one of 3 supported intervals: 3s, 100s, 300s. Timing is managed
by the device and metrics are reported as soon as they become ready. This
measurement mode does include air quality data.

```
pi@airq:~ $ ./metriful-tool cycle-watch
[2020-12-12T02:10:40Z INFO  metriful_tool] waiting for sensor to become ready...
[2020-12-12T02:10:40Z INFO  metriful_tool] metriful sensor is ready
air data:
  temperature:           17.7 ℃
  pressure:              84954 Pa
  humidity:              22.5 % RH
  gas sensor resistance: 29000 Ω

air quality data:
  air quality index: 25
  estimated CO2:     500 ppm
  estimated VOCs:    5 ppm
  AQI accuracy:      invalid

light data:
  illuminance: 9.1 lx
  white level: 163

sound data:
  a-weighted SPL:        55.5 dBa
  SPL frequency bands:   [46.0, 37.8, 41.8, 51.7, 49.1, 45.4]
  peak amplitude:        9522.1 mPa
  measurement stability: unstable

particle data:
  duty cycle:    0 %
  concentration: 0
  validity:      initializing


---
[...]
```

The default interval (3s) can be overridden with `-i <3s|100s|300s>`.

This subcommand supports JSON output with `metriful-tool watch -o json`; JSON
documents are separated by newlines to stdout and can be consumed by e.g. `jq`.

## Cross compiling

This project plays well with [`cross`]. To build for all Raspberry Pis and
similar boards (`arm-unknown-linux-gnueabi`):

```
cross build --target-dir $(pwd)/target-cross --target=arm-unknown-linux-gnueabi --all-features --bins
```

(note: `--target-dir` is recommended to prevent spurious rebuilds when using
both `cargo build` and `cross build`)

Alternatively, the two Dockerfiles have working cross-compiling environments
but are unpleasant to use for development.

[`cross`]: https://github.com/rust-embedded/cross

## Raspberry Pi Setup

Refer to [Metriful's guide][guide] for wiring instructions. Note that the line
`dtparam=i2c_arm=on` must be uncommented in `/boot/config.txt`; the
`raspi-config` utility can do this for you.

In case of GPIO conflicts, the READY pin can be relocated to any free GPIO pin;
the library (and `metriful-tool`) allow arbitrary pin numbers rather than just
the default.

Similarly, in case of a conflict with the default I2C address (`0x71`), the
sensor has a solder bridge which may be closed to use an alternative address
(`0x70`). Both the library and `metriful-tool` support this; refer to the
datasheet for more information.

[guide]: https://github.com/metriful/sensor#use-with-raspberry-pi

## Q&A

### Are particle sensors supported?

Yes, but it's untested. If you have an SDS011, consider connecting the device
directly to the host system (via either USB or UART) and using the
[`sds011-exporter`]. This exports both the PM10 and PM2.5 readings rather than
the single value as reported from the Metriful sensor due to its single PWM
input from the SDS011.

[`sds011-exporter`]: https://github.com/timothyb89/sds011-exporter

### Are interrupts supported?

They cannot currently be configured, however the library can query the interrupt
configuration. See also: `metriful-tool info`

### The device never becomes ready / is always ready and/or read iterators get stuck. What gives?

This can happen if the ready pin is misconfigured; check your pin numbers. Note
that on the Raspberry Pi, GPIO IDs **do not match** pin numbers; refer to the
[GPIO documentation][gpio-docs] for a graphical map of pin numbers to GPIO IDs.

The particular symptoms of this problem vary depending on your host device and
any preexisting GPIO configuration. The simplest way to ensure everything is
configured properly is to use `metriful-tool cycle-watch`, as it will get stuck
on or after the first read if the READY pin is not working properly.

Additionally, if running via `sudo`, be aware that environment variables are
not passed through by default:

```bash
# this won't work
export GPIO_READY=17
sudo metriful-tool cycle-watch

# this will work
sudo GPIO_READY=17 metriful-tool cycle-watch
```

[gpio-docs]: https://www.raspberrypi.org/documentation/usage/gpio/

### Can the library be used asynchronously?

Ultimately the device is single-threaded, however it can be managed via a
background thread. All necessary values are `Send + Sync`, so if desired it
can be configured and handed off to a background thread to report values
asynchronously via a channel.

This is natively supported for cycle reads:

```rust
use std::time::Duration;
use metriful::{Metriful, CyclePeriod, metric::*};

fn main() -> metriful::error::Result<()> {
  let mut metriful = Metriful::try_new(17, "/dev/i2c-1", 0x71)?;
  let (_cmd_tx, metric_rx, _handle) = metriful.async_cycle_read_timeout(
    *METRIC_COMBINED_ALL,
    CyclePeriod::Period0,
    Some(Duration::from_secs(3))
  );

  for metric in metric_rx {
    // ...
  }
}
```
