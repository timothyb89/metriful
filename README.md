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
