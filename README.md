# `metriful-exporter`

A Prometheus exporter and Rust crate for the [Metriful][metriful] sensor.

As it uses [`rust-i2cdev`] and [`rust-sysfs-gpio`], it needs to run on a Linux
host that supports I2C and GPIO, such as the Raspberry Pi.

Requires rustc >= 1.48.

[metriful]: https://github.com/metriful/sensor
[`rust-i2cdev`]: https://github.com/rust-embedded/rust-i2cdev
[`rust-sysfs-gpio`]: https://github.com/rust-embedded/rust-sysfs-gpio

## `metriful-exporter`

`metriful-exporter` serves all Metriful metrics over HTTP as both JSON and
Prometheus metrics.

### Installation

 1. Copy the `metriful-exporter` binary into `/usr/local/bin/`
 2. Create a systemd service file for the exporter at
    `/etc/systemd/system/metriful-exporter.service`:

    ```
    [Unit]
    Description=metriful monitoring service
    After=network.target
    StartLimitIntervalSec=0

    [Service]
    Type=simple
    Restart=always
    RestartSec=1
    User=root
    ExecStart=/usr/local/bin/metriful-exporter --gpio-ready 17 --interval 100s

    [Install]
    WantedBy=multi-user.target
    ```
  3. Enable and start the service: `sudo systemctl enable --now metriful-exporter`

  4. If desired, add a scrape config to your Prometheus instance:

     ```yaml
     - job_name: metriful-office
       scrape_interval: 100s
       static_configs:
         - targets: ['pi.lan:8083']
           labels:
             location: Inside
             room: Office
     ```

     Make sure the scrape interval matches the exporter's interval (either 3,
     100, or 300 seconds)

### API examples

The following examples use [`xh`].

Prometheus metrics: `xh get pi.lan:8083/metrics`: <details><summary>Expand</summary>

```prometheus
metriful_ready 1
metriful_air_gas_sensor_resistance{unit="ohms"} 479736
metriful_air_humidity{unit="% relative humidity"} 17.100000381469727
metriful_air_pressure{unit="pascals"} 84247
metriful_air_temperature{unit="degrees Celsius"} 22
metriful_air_quality_aqi{unit="AQI"} 25
metriful_air_quality_aqi_accuracy{unit="AQI accuracy"} 0
metriful_air_quality_estimated_co2{unit="parts per million"} 500
metriful_air_quality_estimated_voc{unit="parts per million"} 5
metriful_light_illuminance{unit="lux"} 293.5
metriful_light_white_level{unit="white level"} 8249
metriful_sound_measurement_stable{unit="sound measurement stability"} 0
metriful_sound_peak_amplitude{unit="millipascals"} 8489.5
metriful_sound_weighted_spl{unit="A-weighted sound pressure level"} 37.5
metriful_sound_spl_b1{unit="decibels",band_midpoint_hz="125",band_lower_hz="88",band_upper_hz="177"} 38.79999923706055
metriful_sound_spl_b2{unit="decibels",band_midpoint_hz="250",band_lower_hz="177",band_upper_hz="354"} 33.099998474121094
metriful_sound_spl_b3{unit="decibels",band_midpoint_hz="500",band_lower_hz="354",band_upper_hz="707"} 35.099998474121094
metriful_sound_spl_b4{unit="decibels",band_midpoint_hz="1000",band_lower_hz="707",band_upper_hz="1414"} 32.29999923706055
metriful_sound_spl_b5{unit="decibels",band_midpoint_hz="2000",band_lower_hz="1414",band_upper_hz="2828"} 29.399999618530273
metriful_sound_spl_b6{unit="decibels",band_midpoint_hz="4000",band_lower_hz="2828",band_upper_hz="5657"} 26
metriful_read_count 2
metriful_error_count 0
```
</details>

JSON metrics: `xh get pi.lan:8083/json`: <details><summary>Expand</summary>

```json
{
    "error_count": 0,
    "initial_status": {
        "light_int": {
            "status": "disabled"
        },
        "mode": {
            "mode": "standby"
        },
        "particle_sensor": "disabled",
        "sound_int": {
            "status": "disabled"
        }
    },
    "options": {
        "device": "/dev/i2c-1",
        "gpio_ready": 17,
        "i2c_address": 113,
        "interval": {
            "period": "3s"
        },
        "port": 8083,
        "timeout": null
    },
    "read_count": 2,
    "reading": {
        "formatted_value": "air data:\n  temperature:           22 ℃\n  pressure:              84247 Pa\n  humidity:              17.1 % RH\n  gas sensor resistance: 479736 Ω\n\nair quality data:\n  air quality index: 25\n  estimated CO2:     500 ppm\n  estimated VOCs:    5 ppm\n  AQI accuracy:      invalid\n\nlight data:\n  illuminance: 293.5 lx\n  white level: 8249\n\nsound data:\n  a-weighted SPL:        37.5 dBa\n  SPL frequency bands:   [38.8, 33.1, 35.1, 32.3, 29.4, 26.0]\n  peak amplitude:        8489.5 mPa\n  measurement stability: unstable\n\nparticle data:\n  duty cycle:    0 %\n  concentration: 0\n  validity:      initializing\n\n",
        "timestamp": "2021-02-27T22:57:45Z",
        "unit_name": "all combined data",
        "unit_symbol": null,
        "value": {
            "air": {
                "formatted_value": "temperature:           22 ℃\npressure:              84247 Pa\nhumidity:              17.1 % RH\ngas sensor resistance: 479736 Ω\n",
                "timestamp": "2021-02-27T22:57:45Z",
                "unit_name": "combined air data",
                "unit_symbol": null,
                "value": {
                    "gas_sensor_resistance": {
                        "formatted_value": "479736 Ω",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "ohms",
                        "unit_symbol": "Ω",
                        "value": 479736
                    },
                    "humidity": {
                        "formatted_value": "17.1 % RH",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "% relative humidity",
                        "unit_symbol": "% RH",
                        "value": 17.100000381469727
                    },
                    "pressure": {
                        "formatted_value": "84247 Pa",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "pascals",
                        "unit_symbol": "Pa",
                        "value": 84247
                    },
                    "temperature": {
                        "formatted_value": "22 ℃",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "degrees Celsius",
                        "unit_symbol": "℃",
                        "value": 22.0
                    }
                }
            },
            "air_quality": {
                "formatted_value": "air quality index: 25\nestimated CO2:     500 ppm\nestimated VOCs:    5 ppm\nAQI accuracy:      invalid\n",
                "timestamp": "2021-02-27T22:57:45Z",
                "unit_name": "combined air quality data",
                "unit_symbol": null,
                "value": {
                    "aqi": {
                        "formatted_value": "25",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "AQI",
                        "unit_symbol": null,
                        "value": 25.0
                    },
                    "aqi_accuracy": {
                        "formatted_value": "invalid",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "AQI accuracy",
                        "unit_symbol": null,
                        "value": "invalid"
                    },
                    "estimated_co2": {
                        "formatted_value": "500 ppm",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "parts per million",
                        "unit_symbol": "ppm",
                        "value": 500.0
                    },
                    "estimated_voc": {
                        "formatted_value": "5 ppm",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "parts per million",
                        "unit_symbol": "ppm",
                        "value": 5.0
                    }
                }
            },
            "light": {
                "formatted_value": "illuminance: 293.5 lx\nwhite level: 8249\n",
                "timestamp": "2021-02-27T22:57:45Z",
                "unit_name": "combined light data",
                "unit_symbol": null,
                "value": {
                    "illuminance": {
                        "formatted_value": "293.5 lx",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "lux",
                        "unit_symbol": "lx",
                        "value": 293.5
                    },
                    "white_level": {
                        "formatted_value": "8249",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "white level",
                        "unit_symbol": null,
                        "value": 8249
                    }
                }
            },
            "particle": {
                "formatted_value": "duty cycle:    0 %\nconcentration: 0\nvalidity:      initializing\n",
                "timestamp": "2021-02-27T22:57:45Z",
                "unit_name": "combined particle data",
                "unit_symbol": null,
                "value": {
                    "concentration": {
                        "formatted_value": "0",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "raw particle concentration",
                        "unit_symbol": null,
                        "value": {
                            "ppd42_value": 0,
                            "sds011_value": 0.0
                        }
                    },
                    "duty_cycle": {
                        "formatted_value": "0 %",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "percent",
                        "unit_symbol": "%",
                        "value": 0.0
                    },
                    "validity": {
                        "formatted_value": "initializing",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "particle data validity",
                        "unit_symbol": null,
                        "value": "initializing"
                    }
                }
            },
            "sound": {
                "formatted_value": "a-weighted SPL:        37.5 dBa\nSPL frequency bands:   [38.8, 33.1, 35.1, 32.3, 29.4, 26.0]\npeak amplitude:        8489.5 mPa\nmeasurement stability: unstable\n",
                "timestamp": "2021-02-27T22:57:45Z",
                "unit_name": "combined sound data",
                "unit_symbol": null,
                "value": {
                    "measurement_stability": {
                        "formatted_value": "unstable",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "sound measurement stability",
                        "unit_symbol": null,
                        "value": "unstable"
                    },
                    "peak_amplitude": {
                        "formatted_value": "8489.5 mPa",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "millipascals",
                        "unit_symbol": "mPa",
                        "value": 8489.5
                    },
                    "spl_bands": {
                        "formatted_value": "[38.8, 33.1, 35.1, 32.3, 29.4, 26.0]",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "sound pressure level frequency bands",
                        "unit_symbol": null,
                        "value": [
                            38.79999923706055,
                            33.099998474121094,
                            35.099998474121094,
                            32.29999923706055,
                            29.399999618530273,
                            26.0
                        ]
                    },
                    "weighted_spl": {
                        "formatted_value": "37.5 dBa",
                        "timestamp": "2021-02-27T22:57:45Z",
                        "unit_name": "A-weighted sound pressure level",
                        "unit_symbol": "dBa",
                        "value": 37.5
                    }
                }
            }
        }
    }
}
```
</details>

[`xh`]: https://github.com/ducaale/xh

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
