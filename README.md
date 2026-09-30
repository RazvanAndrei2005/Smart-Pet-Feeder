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
