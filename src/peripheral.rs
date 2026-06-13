#![no_main]
#![no_std]

use rmk::macros::rmk_peripheral;

#[path = "ws2812.rs"]
mod ws2812;

#[rmk_peripheral(id = 0)]
mod keyboard_peripheral {
    use embassy_nrf::gpio::{Level, Output, OutputDrive};
    use embassy_nrf::pwm::{Config, Prescaler, SequenceLoad, SequencePwm};

    use crate::ws2812::{PWM_TOP, Role, Ws2812Indicator};

    /// Right half: WS2812 data on P0.13, LED rail (ext-power) on P0.24.
    /// Driven by PWM0 + EasyDMA — no CPU timing loop, so the BLE radio is undisturbed.
    /// Polling processor: it caches device-state events and repaints on a timer.
    #[register_processor(poll)]
    fn rgb() -> Ws2812Indicator {
        let mut config = Config::default();
        config.prescaler = Prescaler::Div1; // 16 MHz PWM clock
        config.max_duty = PWM_TOP; // COUNTERTOP = 20 -> 1.25 us per WS2812 bit
        config.sequence_load = SequenceLoad::Common;
        let pwm = SequencePwm::new_1ch(p.PWM0, p.P0_13, config).unwrap();

        let ext = Output::new(p.P0_24, Level::Low, OutputDrive::Standard);
        // The right half has its own USB-C; charging is derived from its VBUS in
        // the indicator (POWER peripheral), so it shows charging when plugged in too.
        Ws2812Indicator::new(pwm, ext, Role::Peripheral)
    }
}
