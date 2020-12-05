# `metriful-exporter`

A Prometheus exporter and Rust crate for the [Metriful][metriful] sensor.

As it uses [`rust-i2cdev`] and [`rust-sysfs-gpio`], it needs to run on a Linux
host that supports I2C and GPIO, such as the Raspberry Pi.

Requires rustc >= 1.48.

[metriful]: https://github.com/metriful/sensor
[`rust-i2cdev`]: https://github.com/rust-embedded/rust-i2cdev
[`rust-sysfs-gpio`]: https://github.com/rust-embedded/rust-sysfs-gpio

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

They aren't explicitly supported or tested. If you have an SDS011, consider
connecting the device directly to the host system (via either USB or UART) and
using the [`sds011-exporter`]. This exports both the PM10 and PM2.5 readings
rather than the single value as reported from the Metriful sensor due to its
single PWM input from the SDS011.

[`sds011-exporter`]: https://github.com/timothyb89/sds011-exporter

### Are interrupts supported?

Not currently.
