# realtime-gnssrefl
The code repository for Joint Interdisciplinary Project 2025 - Team 3.8.1 TAHMO.

The project contains the following parts

## control
Contains the firmware for the Pico 2. For installation ensure Rust is installed and use the following commands
```bash
$ cd control
$ rustup target add thumbv8m.main-none-eabihf
```
And flash and run the firmware with the following commands:
```bash
$ cd control
$ cargo run --release
```

## analysis
Provides tools for analysing GNSS date (similar to what is in the firmware), and for seeing the results from a firmware dump.

## dashboard
Contains the code for the dashboard. Ensure python is installedand use the following commands:
```bash
$ cd dashboard
$ pip install flask
```
And to start the dashboard run:
```bash
$ cd dashboard
$ python main.py
```

## Design files
Fusion files of the PCB prototype (unfinished). Can also be found here as long as the link is live: https://a360.co/3LjTuod
