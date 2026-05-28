#![no_std]
#![no_main]

use defmt::*;
use embassy_executor::Spawner;
use embassy_rp::gpio::{Input, Level, Output, Pull};
use embassy_rp::i2c::{Config as I2cConfig, I2c};
use embassy_rp::pwm::{Config as PwmConfig, Pwm};
use embassy_rp::spi::{Config as SpiConfig, Spi};
use embassy_rp::pio::Pio;
use embassy_rp::bind_interrupts;
use embassy_time::{Delay, Instant, Timer, Duration};
use embedded_graphics::{
    mono_font::{ascii::FONT_9X15, MonoTextStyle},
    pixelcolor::Rgb565,
    prelude::*,
    text::Text,
};
use hx711::Hx711;
use st7735_lcd::{Orientation, ST7735};
use ds323x::{Ds323x, NaiveDate, DateTimeAccess, Timelike};
use {defmt_rtt as _, panic_probe as _};

use aligned::{Aligned, A4};
use cyw43_pio::{PioSpi, RM2_CLOCK_DIVIDER};
use embassy_net::tcp::TcpSocket;
use embassy_net::{Config, Stack, StackResources};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::signal::Signal;
use embedded_io_async::Write;
use static_cell::StaticCell;
use core::sync::atomic::{AtomicU32, Ordering};

use embedded_hal_bus::spi::ExclusiveDevice;
use embedded_sdmmc::{SdCard, VolumeManager, Mode, VolumeIdx, TimeSource, Timestamp};

static WEB_FEED_SIGNAL: Signal<CriticalSectionRawMutex, bool> = Signal::new();
static CURRENT_WEIGHT: AtomicU32 = AtomicU32::new(0);

bind_interrupts!(struct Irqs {
    PIO0_IRQ_0 => embassy_rp::pio::InterruptHandler<embassy_rp::peripherals::PIO0>;
    DMA_IRQ_0  => embassy_rp::dma::InterruptHandler<embassy_rp::peripherals::DMA_CH0>;
});

static FW:  &Aligned<A4, [u8]> = &Aligned(*include_bytes!("../firmware/43439A0.bin"));
static CLM: &Aligned<A4, [u8]> = &Aligned(*include_bytes!("../firmware/43439A0_clm.bin"));
static NVRAM: &Aligned<A4, [u8]> = &Aligned(*include_bytes!("../firmware/nvram_rp2040.bin"));

struct SdClock;
impl TimeSource for SdClock {
    fn get_timestamp(&self) -> Timestamp {
        Timestamp {
            year_since_1970: 56, 
            zero_indexed_month: 0,
            zero_indexed_day: 0,
            hours: 12,
            minutes: 0,
            seconds: 0,
        }
    }
}

#[embassy_executor::task]
async fn wifi_task(
    runner: cyw43::Runner<
        'static,
        cyw43::SpiBus<
            Output<'static>,
            PioSpi<'static, embassy_rp::peripherals::PIO0, 0>,
        >,
    >,
) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn net_task(mut runner: embassy_net::Runner<'static, cyw43::NetDriver<'static>>) -> ! {
    runner.run().await
}

#[embassy_executor::task]
async fn web_server_task(stack: &'static Stack<'static>) {
    let mut rx_buffer = [0u8; 4096];
    let mut tx_buffer = [0u8; 4096];

    loop {
        let mut socket = TcpSocket::new(*stack, &mut rx_buffer, &mut tx_buffer);
        socket.set_timeout(Some(Duration::from_secs(10)));

        info!("Listening for phone app on Port 80...");
        if socket.accept(80).await.is_ok() {
            info!("Phone connected!");

            let mut buf = [0u8; 1024];
            if let Ok(n) = socket.read(&mut buf).await {
                let request = core::str::from_utf8(&buf[..n]).unwrap_or("");

                if request.contains("GET /feed") {
                    info!("📲 REMOTE FEED TRIGGERED BY PHONE!");
                    WEB_FEED_SIGNAL.signal(true);

                    let response =
                        b"HTTP/1.1 200 OK\r\n\
                        Content-Type: text/plain\r\n\
                        Connection: close\r\n\
                        \r\n\
                        OK";
                    let _ = socket.write_all(response).await;

                } else if request.contains("GET /weight") {
                    let weight = CURRENT_WEIGHT.load(Ordering::Relaxed);
                    let whole = weight / 10;
                    let decimal = weight % 10;
                    let mut response_buf = [0u8; 128];

                    let response = format_no_std::show(
                        &mut response_buf,
                        format_args!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: text/plain\r\n\
                             Connection: close\r\n\
                             \r\n\
                             {}.{}",
                            whole, decimal
                        )
                    ).unwrap();
                    let _ = socket.write_all(response.as_bytes()).await;

                } else if request.contains("GET / ") {
                    let html = include_str!("index.html");
                    let mut response_buf = [0u8; 4096];

                    let response = format_no_std::show(
                        &mut response_buf,
                        format_args!(
                            "HTTP/1.1 200 OK\r\n\
                             Content-Type: text/html\r\n\
                             Connection: close\r\n\
                             \r\n\
                             {}",
                            html
                        )
                    ).unwrap();
                    let _ = socket.write_all(response.as_bytes()).await;
                }
            }
        }
        let _ = socket.flush().await;
        socket.close();
    }
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let p = embassy_rp::init(Default::default());
    info!("🚀 SYSTEM BOOTING: IOT EDITION...");

    let pwr = Output::new(p.PIN_23, Level::Low);
    let cs  = Output::new(p.PIN_25, Level::High);
    let mut pio = Pio::new(p.PIO0, Irqs);
    let spi = PioSpi::new(
        &mut pio.common,
        pio.sm0,
        RM2_CLOCK_DIVIDER,
        pio.irq0,
        cs,              
        p.PIN_24,        
        p.PIN_29,        
        embassy_rp::dma::Channel::new(p.DMA_CH0, Irqs), 
    );

    static STATE: StaticCell<cyw43::State> = StaticCell::new();
    let state = STATE.init(cyw43::State::new());

    let (net_device, mut control, runner) = cyw43::new(state, pwr, spi, FW, NVRAM).await;
    spawner.spawn(wifi_task(runner).unwrap());

    control.set_power_management(cyw43::PowerManagementMode::Performance).await;
    control.init(CLM).await;

    let ssid     = "Test1";
    let password = "11220011";
    let dhcp_config = Config::dhcpv4(Default::default());

    static STACK_RESOURCES: StaticCell<StackResources<2>> = StaticCell::new();
    static STACK: StaticCell<Stack<'static>> = StaticCell::new();

    let (stack, net_runner) = embassy_net::new(
        net_device, dhcp_config, STACK_RESOURCES.init(StackResources::<2>::new()), 1234, 
    );
    let stack: &'static Stack<'static> = STACK.init(stack);
    spawner.spawn(net_task(net_runner).unwrap());

    info!("Connecting to Hotspot '{}'...", ssid);
    loop {
        match control.join(ssid, cyw43::JoinOptions::new(password.as_bytes())).await {
            Ok(_) => break,
            Err(_) => Timer::after_secs(2).await,
        }
    }
    info!("✅ Connected to Wi-Fi!");

    while !stack.is_config_up() { Timer::after_millis(500).await; }
    spawner.spawn(web_server_task(stack).unwrap());

    Timer::after_secs(1).await;
    let dc  = Output::new(p.PIN_5, Level::Low);
    let rst = Output::new(p.PIN_6, Level::Low);
    let _cs = Output::new(p.PIN_4, Level::Low);
    let mut spi0_config = SpiConfig::default();
    spi0_config.frequency = 10_000_000;
    let spi0 = Spi::new_blocking(p.SPI0, p.PIN_2, p.PIN_3, p.PIN_0, spi0_config);
    let mut display = ST7735::new(spi0, dc, rst, false, false, 128, 128);
    display.init(&mut Delay).unwrap();
    display.set_orientation(&Orientation::Portrait).unwrap();
    display.clear(Rgb565::BLACK).unwrap();
    let style = MonoTextStyle::new(&FONT_9X15, Rgb565::WHITE);
    info!("✅ Display initialized.");

    let mut spi1_config = SpiConfig::default();
    spi1_config.frequency = 400_000; 
    let spi1 = Spi::new_blocking(p.SPI1, p.PIN_26, p.PIN_27, p.PIN_28, spi1_config);
    let sd_cs = Output::new(p.PIN_9, Level::High);
    
    let spi_device = ExclusiveDevice::new_no_delay(spi1, sd_cs).unwrap(); 
    
    let sdcard = SdCard::new(spi_device, Delay);
    let mut volume_mgr = VolumeManager::new(sdcard, SdClock);
    info!("✅ MicroSD Controller initialized on SPI1.");
    Timer::after_secs(1).await;
    let i2c = I2c::new_blocking(p.I2C0, p.PIN_17, p.PIN_16, I2cConfig::default());
    let mut rtc = Ds323x::new_ds3231(i2c);
    info!("✅ RTC initialized.");

    let mut pwm_config: PwmConfig = Default::default();
    pwm_config.top = 20000;
    pwm_config.divider = 150.into();
    let mut pwm = Pwm::new_output_b(p.PWM_SLICE7, p.PIN_15, pwm_config.clone());

    let mut trig     = Output::new(p.PIN_14, Level::Low);
    let echo         = Input::new(p.PIN_13, Pull::None);
    let hx711_dt     = Input::new(p.PIN_12, Pull::None);
    let hx711_sck    = Output::new(p.PIN_11, Level::Low);
    let mut scale    = Hx711::new(Delay, hx711_dt, hx711_sck).unwrap();
    let pir          = Input::new(p.PIN_10, Pull::Down);
    let button       = Input::new(p.PIN_18, Pull::Up);

    let mut led_green  = Output::new(p.PIN_20, Level::Low);
    let mut led_yellow = Output::new(p.PIN_21, Level::Low);
    let mut led_red    = Output::new(p.PIN_22, Level::Low);

    info!("Taring scale — keep load cell unloaded...");
    let mut tare_sum: i32 = 0;
    for _ in 0..8 {
        tare_sum += nb::block!(scale.retrieve()).unwrap();
        Timer::after_millis(120).await;
    }
    let tare_offset: i32 = tare_sum / 8;
    const COUNTS_PER_GRAM: f32 = 450.0;
    info!("✅ System Fully Online!");

    let mut servo_is_open   = false;
    let mut weight_grams: f32 = 0.0;
    let mut expected_weight: f32 = 0.0;
    let mut is_error_state  = false;
    let mut feed_in_progress = false;
    let mut feed_timer      = Instant::now();
    let mut fed_morning     = false;
    let mut fed_evening     = false;
    let mut last_pir_state  = false;
    let mut cat_on_screen_timer: u8 = 0; 

    loop {
        let current_time = match rtc.datetime() {
            Ok(dt)  => dt,
            Err(_)  => NaiveDate::from_ymd_opt(2000, 1, 1).unwrap().and_hms_opt(0, 0, 0).unwrap(),
        };

        if current_time.hour() == 8 && current_time.minute() == 0 && !fed_morning {
            trigger_feed(&mut pwm, &mut pwm_config, &mut servo_is_open, &mut expected_weight, &mut feed_in_progress, &mut feed_timer, weight_grams);
            fed_morning = true;
        }
        if current_time.hour() == 18 && current_time.minute() == 0 && !fed_evening {
            trigger_feed(&mut pwm, &mut pwm_config, &mut servo_is_open, &mut expected_weight, &mut feed_in_progress, &mut feed_timer, weight_grams);
            fed_evening = true;
        }
        if current_time.hour() == 0 {
            fed_morning = false;
            fed_evening = false;
        }

        let current_pir_state = pir.is_high();
        if current_pir_state && !last_pir_state {
            info!("Cat detected at {:02}:{:02}:{:02}", current_time.hour(), current_time.minute(), current_time.second());
            cat_on_screen_timer = 25; 

            if let Ok(mut volume) = volume_mgr.open_volume(VolumeIdx(0)) {
                if let Ok(mut dir) = volume.open_root_dir() {
                    if let Ok(mut file) = dir.open_file_in_dir("CATLOG.TXT", Mode::ReadWriteCreateOrAppend) {
                        
                        let mut log_buf = [0u8; 64];
                        if let Some(log_str) = format_no_std::show(&mut log_buf, format_args!("Cat detected at {:02}:{:02}:{:02}\r\n", current_time.hour(), current_time.minute(), current_time.second())) {
                            match file.write(log_str.as_bytes()) {
                                Ok(_) => info!("✅ Successfully saved to CATLOG.TXT!"),
                                Err(_) => info!("❌ File opened, but writing failed!"),
                            }
                        }
                    }
                }
            } else {
                info!("⚠️ Failed to write to SD Card (Not inserted or unformatted?)");
            }
        }
        last_pir_state = current_pir_state;

        weight_grams = match nb::block!(scale.retrieve()) {
            Ok(raw) => {
                let calculated = (raw - tare_offset) as f32 / COUNTS_PER_GRAM;
                if calculated < 0.0 { 0.0 } else { calculated }
            },
            Err(_)  => f32::NAN,
        };
        CURRENT_WEIGHT.store((weight_grams * 10.0) as u32, Ordering::Relaxed);

        if button.is_low() || WEB_FEED_SIGNAL.try_take().is_some() {
            trigger_feed(&mut pwm, &mut pwm_config, &mut servo_is_open, &mut expected_weight, &mut feed_in_progress, &mut feed_timer, weight_grams);
            Timer::after_millis(300).await;
        }

        if feed_in_progress {
            let elapsed_ms = feed_timer.elapsed().as_millis();
            if elapsed_ms >= 300 && servo_is_open {
                pwm_config.compare_b = 1000;
                pwm.set_config(&pwm_config);
                servo_is_open = false;
            }
            if elapsed_ms >= 2000 {
                feed_in_progress = false;
                if weight_grams < expected_weight {
                    is_error_state = true;
                } else {
                    is_error_state = false;
                }
            }
        }

        if is_error_state && weight_grams >= expected_weight {
            is_error_state = false;
        }

        trig.set_low(); Timer::after_micros(2).await;
        trig.set_high(); Timer::after_micros(10).await;
        trig.set_low();

        let mut distance_cm = 0.0_f32;
        let mut timeout = 0u32;
        while echo.is_low() && timeout < 10_000 { timeout += 1; }
        if timeout < 10_000 {
            let start = Instant::now();
            timeout = 0;
            while echo.is_high() && timeout < 10_000 { timeout += 1; }
            let end = Instant::now();
            distance_cm = (end.duration_since(start).as_micros() as f32 * 0.0343) / 2.0;
        }

        if is_error_state {
            led_red.set_high(); led_green.set_low(); led_yellow.set_low();
        } else {
            led_red.set_low();
            if distance_cm > 0.0 && distance_cm < 15.0 {
                led_green.set_high(); led_yellow.set_low();
            } else {
                led_green.set_low(); led_yellow.set_high();
            }
        }

        display.clear(Rgb565::BLACK).unwrap();
        let mut time_buf = [0u8; 16];
        if let Some(s) = format_no_std::show(&mut time_buf, format_args!("{:02}:{:02}", current_time.hour(), current_time.minute())) {
            Text::new(s, Point::new(2, 15), style).draw(&mut display).unwrap();
        }
        let mut scr_dist = [0u8; 16];
        if let Some(s) = format_no_std::show(&mut scr_dist, format_args!("Dist: {:.1}cm", distance_cm)) {
            Text::new(s, Point::new(2, 40), style).draw(&mut display).unwrap();
        }
        let mut scr_wt = [0u8; 16];
        if let Some(s) = format_no_std::show(&mut scr_wt, format_args!("Wt: {:.1}g", weight_grams)) {
            Text::new(s, Point::new(2, 60), style).draw(&mut display).unwrap();
        }
        
        if cat_on_screen_timer > 0 {
            Text::new("CAT DETECTED!", Point::new(2, 90), style).draw(&mut display).unwrap();
            cat_on_screen_timer -= 1;
        }

        Timer::after_millis(200).await;
    }
}

fn trigger_feed(
    pwm:         &mut Pwm<'_>,
    config:      &mut PwmConfig,
    is_open:     &mut bool,
    expected_wt: &mut f32,
    in_prog:     &mut bool,
    timer:       &mut Instant,
    current_wt:  f32,
) {
    info!("Feeding triggered! Opening servo...");
    config.compare_b = 2000;
    pwm.set_config(config);
    *is_open = true;
    
    *expected_wt = current_wt + 3.0;
    *in_prog = true;
    *timer   = Instant::now();
}

mod format_no_std {
    use core::fmt;
    pub fn show<'a>(buf: &'a mut [u8], args: fmt::Arguments) -> Option<&'a str> {
        let mut w = Wrapper(buf, 0);
        fmt::write(&mut w, args).ok()?;
        core::str::from_utf8(&w.0[..w.1]).ok()
    }
    struct Wrapper<'a>(&'a mut [u8], usize);
    impl<'a> fmt::Write for Wrapper<'a> {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let bytes = s.as_bytes();
            let rem   = &mut self.0[self.1..];
            if bytes.len() > rem.len() { return Err(fmt::Error); }
            rem[..bytes.len()].copy_from_slice(bytes);
            self.1 += bytes.len();
            Ok(())
        }
    }
}