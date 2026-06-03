#![no_main]
#![no_std]

use rmk::macros::rmk_central;

#[path = "ws2812.rs"]
mod ws2812;

#[rmk_central]
mod keyboard_central {
    use embassy_nrf::gpio::{Level, Output, OutputDrive};
    use embassy_nrf::pwm::{Config, Prescaler, SequenceLoad, SequencePwm};
    // Bring the polling-loop trait into the generated `main` so the controller
    // driver the macro emits (`rgb.polling_loop()`) resolves.
    use rmk::controller::PollingController;

    use crate::ws2812::{PWM_TOP, Role, Ws2812Indicator};

    /// Left half: WS2812 data on P0.24, LED rail (ext-power) on P0.13.
    /// Driven by PWM0 + EasyDMA — no CPU timing loop, so the BLE radio is undisturbed.
    #[controller(poll)]
    fn rgb() -> Ws2812Indicator {
        let mut config = Config::default();
        config.prescaler = Prescaler::Div1; // 16 MHz PWM clock
        config.max_duty = PWM_TOP; // COUNTERTOP = 20 -> 1.25 us per WS2812 bit
        config.sequence_load = SequenceLoad::Common;
        let pwm = SequencePwm::new_1ch(p.PWM0, p.P0_24, config).unwrap();

        let ext = Output::new(p.P0_13, Level::Low, OutputDrive::Standard);
        Ws2812Indicator::new(pwm, ext, Role::Central)
    }
}
