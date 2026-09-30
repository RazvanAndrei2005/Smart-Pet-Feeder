# 🐾 Smart Pet Feeder

A smart, automated pet feeder that reliably dispenses perfectly portioned meals on schedule while keeping track of your pet's daily eating routines. Designed with fail-safes and data logging, this system actively monitors for food jams to ensure your pet is always fed and automatically logs exactly when they visit the bowl.

## ✨ Features

* **Precise Portion Control:** Uses a load cell to dispense exact gram-weight portions, eliminating overfeeding or underfeeding.
* **Offline Reliability:** High-precision RTC ensures feeding schedules execute flawlessly even if the internet goes down.
* **Active Jam Detection:** Feedback loops monitor dispensing weight and motor strain, automatically reversing the mechanism if a food jam occurs.
* **Habit Tracking:** PIR presence detection logs every time your pet visits the bowl, saving timestamps to a MicroSD card.
* **Web Dashboard:** Embedded async web server allows you to view feeding logs, monitor system health, and trigger manual feeds from your phone.
* **Local UI:** 1.44" Color LCD with status LEDs provides immediate visual feedback on system state.

## 🏗️ Architecture

The system is built on an asynchronous, event-driven architecture using Rust and the Embassy framework, running on a Raspberry Pi Pico 2 W. 

```text
  +------------------+             +-------------------+             +-----------------------+
  |  Time Manager    |====I2C======|                   |====PWM======| Dispensing Mechanism  |
  |  (RTC Module)    |             |                   |             | (Servo Motor)         |
  +------------------+             |                   |             +-----------------------+
                                   |                   |
  +------------------+             |                   |             +-----------------------+
  |  Weight Monitor  |===Data======|                   |====SPI======|  Visual Interface     |
  |  (Load Cell/ADC) |             |  State Manager &  |             |  (Color Display)      |
  +------------------+             |  Logic Controller |             +-----------------------+
                                   |   (Pico 2W)       |
  +------------------+             |                   |             +-----------------------+
  | Presence Tracker |===GPIO======|                   |====GPIO=====| Status Indicators     |
  | (PIR Sensor)     |             |                   |             | (LEDs & Buzzer)       |
  +------------------+             |                   |             +-----------------------+
                                   |                   |
  +------------------+             |                   |             [Data Subsystem]
  | Local Input      |===GPIO======|                   |             +-----------------------+
  | (Push Buttons)   |             +-------------------+====SPI======|  Persistence Logger   |
  +------------------+                                               |  (MicroSD Module)     |
                                                                     +-----------------------+
```

### Core Subsystems
* **Core Controller:** The Rust-based central brain that manages the state machine, evaluates feeding schedules, and processes sensor data concurrently.
* **Perception Subsystem:** Handles real-time inputs: offline timekeeping (RTC), exact weight monitoring (ADC), pet presence detection (PIR), and manual interactions.
* **Actuation & UI Subsystem:** Drives the physical food-dispensing servo motor and provides real-time feedback via the color screen, LEDs, and buzzer.
* **Data Persistence:** Manages non-volatile storage, automatically logging feeding success events and visitation timestamps to the MicroSD card.

## 🛠️ Hardware Requirements

| Component | Purpose |
|-----------|---------|
| **Raspberry Pi Pico 2 W** | Main microcontroller handling async logic and Wi-Fi capabilities. |
| **DS3231 RTC Module** | High-precision I2C offline clock for scheduled feeds. |
| **1/5 kg Load Cell + HX711** | High-precision weight monitoring for exact portion control. |
| **SG90 Micro Servomotor** | Actuates the food dispensing mechanism. |
| **HC-SR505 PIR Sensor** | Mini motion detector to log when the pet visits the bowl. |
| **1.44" SPI Color Display** | Local UI powered by an ST7735 controller. |
| **MicroSD Card & Module** | Persistent storage for feeding history and presence logs. |
| **HC-SR04+ Ultrasonic** | Distance sensor to monitor the remaining food level in the hopper. |
| **Misc. Electronics** | Push buttons, LEDs, Buzzer, 220Ω resistors, capacitors, and jumper wires. |

## 💻 Software Stack

The firmware is written entirely in **Rust** using the `embassy` framework for safe, concurrent, and asynchronous embedded execution.

| Crate / Library | Role in Project |
|-----------------|-----------------|
| `embassy-rp` / `embassy-executor` | Async Hardware Abstraction Layer and task executor for concurrent operations. |
| `embassy-net` / `cyw43` | Embedded TCP/IP network stack and Wi-Fi chip drivers for network connectivity. |
| `picoserve` | Async embedded web server for the local dashboard and manual control. |
| `hx711` | Driver for the HX711 ADC to read real-time weight values for portion control. |
| `ds323x` | I2C driver for highly accurate offline timekeeping. |
| `st7735-lcd` & `embedded-graphics` | Display driver and 2D graphics library for rendering the local UI. |
| `embedded-sdmmc` | SPI-based SD Card & FAT16/FAT32 library for persistent data logging. |
| `fixed` | Fixed-point arithmetic for precise, lightweight math without float overhead. |
| `defmt` & `panic-probe` | Deferred formatting and highly efficient logging framework for debugging. |

## 🚀 Getting Started

### Prerequisites
* Rust toolchain (nightly may be required for certain async embedded features)
* `probe-rs` for flashing and debugging
* Hardware assembled according to the architecture schematic

### Building and Flashing
```bash
# Clone the repository
git clone https://github.com/UPB-PMRust-Students/fils-project-2026-RazvanAndrei2005.git
cd fils-project-2026-RazvanAndrei2005

# Build the project
cargo build --release

# Flash to the Pico 2 W
cargo run --release
```

## 📚 References & Inspiration
* [Hackaday: DIY Automated Pet Feeder Projects](https://hackaday.com/tag/pet-feeder/)
* [Printables: 3D Printed Auger and Dispenser Mechanisms](https://www.printables.com/search/models?q=pet%20feeder)
* Commercial Inspiration: [Petlibro](https://petlibro.com/collections/automatic-pet-feeder)

---
**Author:** Borca Razvan Andrei
