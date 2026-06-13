//! WS2812 status indicator for the Cornix.
//!
//! Each half carries two serial RGB LEDs, driven by the nRF52840 **PWM
//! peripheral with EasyDMA**. Each WS2812 data bit is one PWM period and the
//! whole 2-pixel frame is a sequence the PWM hardware clocks out by DMA. The
//! CPU is not in the timing loop at all: no interrupt masking, no busy-wait —
//! so the BLE radio (nrf-sdc / MPSL) is never disturbed. (Earlier CPU
//! bit-bang + `interrupt::free` approaches starved the radio and froze the
//! link; a SPIM attempt produced no output on this board. PWM is the clean,
//! hardware-timed way.)
//!
//! The two pixels are:
//!
//!   * pixel 0 — "inner": battery / charging / peer-loss notifications
//!   * pixel 1 — "outer": Bluetooth profile (central) or peer link (peripheral)
//!
//! The indicator is an RMK [`PollingProcessor`]: the `#[processor]` macro wires
//! it up to the device-state event channels, caches the latest values in the
//! `on_*_event` handlers, and calls [`Ws2812Indicator::poll`] every
//! `poll_interval` ms to repaint both pixels. All animation is derived from a
//! frame counter, so no extra timers are needed. State-change events reset the
//! counter, which is how the one-shot "show for N frames then go dark" pulses
//! (host-connect, peer-up, fully-charged) are timed. [`render`] skips the DMA
//! transfer when the frame is unchanged — the WS2812 latch holds the last
//! colour — so an idle indicator does no work at all.

use embassy_nrf::gpio::Output;
use embassy_nrf::pwm::{SequenceConfig, SequencePwm, SingleSequenceMode, SingleSequencer};
use embassy_time::{Duration, Timer};
use rmk::event::{
    BatteryStatusEvent, CentralConnectedEvent, ChargingStateEvent, ConnectionStatusChangeEvent,
    LedIndicatorEvent, PeripheralConnectedEvent,
};
use rmk::macros::processor;
use rmk::types::battery::BatteryStatus;
use rmk::types::ble::BleState;

/// Which half this indicator runs on. Determines the meaning of the outer pixel
/// and which "link lost" signal feeds the peer-loss blink.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Central,
    Peripheral,
}

/// Frames per breathing period (~2 s).
const BREATH_FRAMES: u32 = 60;
/// Frames in one blink period (~1 s) and the "on" portion (~0.4 s, 40% duty).
const BLINK_PERIOD: u32 = 30;
const BLINK_ON: u32 = 12;
/// Number of blink cycles before a "link lost" / "searching" blink gives up.
const BLINK_MAX_CYCLES: u32 = 10;

/// One-shot display windows, in frames (~33 ms each). These give the
/// "pulse then off" behaviour: the LED lights on a state change and goes dark
/// again so the strip is not left steadily on during normal use.
const CONNECT_SHOW_FRAMES: u32 = 75; // ~2.5 s host-connect pulse (central outer)
const PEER_SHOW_FRAMES: u32 = 90; // ~3 s peer-link-up pulse
const FULL_SHOW_FRAMES: u32 = 90; // ~3 s fully-charged green

/// Dim steady level, and the breathing peak.
const LEVEL: u8 = 0x10;
const BREATH_PEAK: u8 = 0x20;

/// Battery percentage at/under which the low-battery warning shows.
const BATTERY_LOW: u8 = 20;
/// Battery percentage at/over which charging is treated as "full".
const BATTERY_FULL: u8 = 95;

// WS2812 encoding for the PWM peripheral. The PWM runs at 16 MHz (Div1) with a
// COUNTERTOP of 20, so one period is 20 ticks = 1.25 µs — exactly one WS2812
// bit. Each sequence word is an embassy `DutyCycle` raw value: bit 15 set
// (inverted polarity) makes the output HIGH for the low 15-bit count of ticks
// at the start of the period, then LOW. So `0x8000 | n` = HIGH for n/20 of the
// period:
//   `0` bit: 6 ticks high  ≈ 0.375 µs        `1` bit: 13 ticks high ≈ 0.81 µs
pub const PWM_TOP: u16 = 20;
const W0: u16 = 0x8000 | 6;
const W1: u16 = 0x8000 | 13;
/// All-low word (HIGH for 0 ticks) used for the >50 µs reset/latch tail.
const WRESET: u16 = 0x8000;
/// 2 pixels × 3 colour bytes × 8 bits.
const SEQ_BITS: usize = 2 * 3 * 8;
/// Reset/latch words: ~40 × 1.25 µs ≈ 50 µs of low after the data.
const SEQ_RESET: usize = 40;
const SEQ_LEN: usize = SEQ_BITS + SEQ_RESET;

/// 1 - cos breathing curve over BREATH_FRAMES samples, scaled to BREATH_PEAK.
const fn breath_table() -> [u8; BREATH_FRAMES as usize] {
    // cos approximated by a symmetric triangle is visually close enough and
    // keeps this const; the ramp goes 0 -> peak -> 0 with no plateau.
    let mut t = [0u8; BREATH_FRAMES as usize];
    let half = BREATH_FRAMES / 2;
    let mut i = 0u32;
    while i < BREATH_FRAMES {
        let up = if i <= half { i } else { BREATH_FRAMES - i };
        t[i as usize] = ((up * BREATH_PEAK as u32) / half) as u8;
        i += 1;
    }
    t
}
static BREATH: [u8; BREATH_FRAMES as usize] = breath_table();

/// A single pixel colour in the WS2812 GRB transmission order.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct Grb {
    g: u8,
    r: u8,
    b: u8,
}

const OFF: Grb = Grb { g: 0, r: 0, b: 0 };
const RED: Grb = Grb { g: 0, r: LEVEL, b: 0 };
const GREEN: Grb = Grb { g: LEVEL, r: 0, b: 0 };
const BLUE: Grb = Grb { g: 0, r: 0, b: LEVEL };

/// Status indicator processor.
///
/// `#[processor]` generates the `Processor` / `PollingProcessor` / `Runnable`
/// impls: it subscribes to the listed event channels, dispatches each event to
/// the matching `on_<event>_event` handler below, and calls [`Self::poll`]
/// every `poll_interval` ms. The subscribe list is the union of what both
/// halves need; events that never fire on a given half are simply never
/// delivered (each half is its own binary, but the struct is shared).
#[processor(
    subscribe = [
        BatteryStatusEvent,
        ChargingStateEvent,
        PeripheralConnectedEvent,
        CentralConnectedEvent,
        ConnectionStatusChangeEvent,
        LedIndicatorEvent,
    ],
    poll_interval = 33
)]
pub struct Ws2812Indicator {
    pwm: SequencePwm<'static>,
    ext_power: Output<'static>,
    role: Role,

    // Cached device state. `battery` is `None` until the first reading so a
    // not-yet-known level is never mistaken for "full" (>= BATTERY_FULL) or
    // "low" (<= BATTERY_LOW) — which at boot would flash the wrong colour.
    battery: Option<u8>,
    charging: bool,
    ble_profile: u8,
    ble_connected: bool,
    ble_advertising: bool,
    peer_connected: bool,

    // Animation bookkeeping.
    frame: u32,
    rail_on: bool,
    // Last colours pushed to the strip; used to skip redundant DMA transfers.
    last: Option<(Grb, Grb)>,
}

impl Ws2812Indicator {
    pub fn new(pwm: SequencePwm<'static>, mut ext_power: Output<'static>, role: Role) -> Self {
        // Keep the LED rail off until we have something to show.
        ext_power.set_low();
        Self {
            pwm,
            ext_power,
            role,
            battery: None,
            charging: false,
            ble_profile: 0,
            ble_connected: false,
            ble_advertising: false,
            peer_connected: false,
            frame: 0,
            rail_on: false,
            last: None,
        }
    }

    /// Colour of the active Bluetooth profile (central outer pixel).
    fn profile_color(&self) -> Grb {
        match self.ble_profile {
            0 => GREEN,
            1 => RED,
            _ => BLUE,
        }
    }

    /// True for the "on" window of the current blink cycle, while still within
    /// the cycle cap.
    fn blink_on(&self) -> bool {
        let cycle = self.frame / BLINK_PERIOD;
        cycle < BLINK_MAX_CYCLES && (self.frame % BLINK_PERIOD) < BLINK_ON
    }

    /// True for the brief double-blink window used by the low-battery warning.
    fn double_blink_on(&self) -> bool {
        let phase = self.frame % BLINK_PERIOD;
        phase < 6 || (phase >= 12 && phase < 18)
    }

    /// Update the cached charging flag, restarting the animation on a change so
    /// the charge pulse / fully-charged window time from the transition.
    fn set_charging(&mut self, charging: bool) {
        if charging != self.charging {
            self.charging = charging;
            self.frame = 0;
        }
    }

    fn inner_color(&self) -> Grb {
        // Charging takes priority and lives on the inner pixel.
        if self.charging {
            if self.battery.is_some_and(|b| b >= BATTERY_FULL) {
                // Fully charged: green for a few seconds, then dark. Only when
                // the level is actually known — an unknown level breathes.
                return if self.frame < FULL_SHOW_FRAMES { GREEN } else { OFF };
            }
            let level = BREATH[(self.frame % BREATH_FRAMES) as usize];
            return Grb { g: level, r: 0, b: 0 };
        }
        // Peer link is shown on the inner pixel for the central only (the
        // peripheral shows it on its outer pixel instead).
        if self.role == Role::Central {
            if !self.peer_connected {
                // Peer lost: slow blue blink (capped).
                return if self.blink_on() { BLUE } else { OFF };
            } else if self.frame < PEER_SHOW_FRAMES {
                // Peer just linked: steady blue pulse, then dark.
                return BLUE;
            }
        }
        // Low battery: red double-blink warning (only on a known level).
        if self.battery.is_some_and(|b| b <= BATTERY_LOW) {
            return if self.double_blink_on() { RED } else { OFF };
        }
        OFF
    }

    fn outer_color(&self) -> Grb {
        match self.role {
            Role::Central => {
                if self.ble_connected {
                    // Host just connected: flash the profile colour, then dark.
                    if self.frame < CONNECT_SHOW_FRAMES {
                        self.profile_color()
                    } else {
                        OFF
                    }
                } else if self.ble_advertising {
                    // Searching: slow blink in the profile colour (capped).
                    if self.blink_on() { self.profile_color() } else { OFF }
                } else {
                    OFF
                }
            }
            Role::Peripheral => {
                // Outer pixel mirrors the central link state.
                if self.peer_connected {
                    // Central just linked: steady blue pulse, then dark.
                    if self.frame < PEER_SHOW_FRAMES { BLUE } else { OFF }
                } else if self.blink_on() {
                    BLUE
                } else {
                    OFF
                }
            }
        }
    }

    /// Encode both pixels (GRB, MSB first) into the PWM sequence buffer, with a
    /// trailing low tail for the WS2812 reset/latch.
    fn encode(buf: &mut [u16; SEQ_LEN], inner: Grb, outer: Grb) {
        let bytes = [inner.g, inner.r, inner.b, outer.g, outer.r, outer.b];
        let mut k = 0;
        for byte in bytes {
            let mut b = byte;
            for _ in 0..8 {
                buf[k] = if b & 0x80 != 0 { W1 } else { W0 };
                k += 1;
                b <<= 1;
            }
        }
        while k < SEQ_LEN {
            buf[k] = WRESET;
            k += 1;
        }
    }

    /// Render both pixels by DMA-clocking one PWM sequence, gating the LED power
    /// rail so it is only powered while something is lit. Skips the transfer
    /// when nothing changed since the last frame — the WS2812 latch holds the
    /// previous colour — so an idle indicator does nothing.
    async fn render(&mut self, inner: Grb, outer: Grb) {
        if self.last == Some((inner, outer)) {
            return;
        }
        self.last = Some((inner, outer));

        let any_on = inner != OFF || outer != OFF;

        if any_on && !self.rail_on {
            // Bring the rail up and give the strip a moment to settle before
            // the first frame so it latches a clean colour.
            self.ext_power.set_high();
            Timer::after(Duration::from_millis(5)).await;
            self.rail_on = true;
        }

        let mut buf = [WRESET; SEQ_LEN];
        Self::encode(&mut buf, inner, outer);
        {
            let seq = SingleSequencer::new(&mut self.pwm, &buf, SequenceConfig::default());
            if seq.start(SingleSequenceMode::Times(1)).is_ok() {
                // Let the DMA finish before `seq` (and `buf`) drop. The whole
                // sequence is ~ SEQ_LEN × 1.25 µs ≈ 110 µs; 1 ms is ample.
                Timer::after(Duration::from_millis(1)).await;
            }
        }

        if !any_on {
            // We just pushed an all-off frame to blank the strip; now cut power.
            self.ext_power.set_low();
            self.rail_on = false;
        }
    }

    // --- Event handlers (dispatched by the `#[processor]` macro) ---

    /// Local battery status: tracks the level only. The charging flag is driven
    /// from USB VBUS in `poll`, not from this event's `charge_state` — that
    /// field stays `Discharging`/`Unknown` here because RMK never publishes a
    /// real charging transition in this revision. The `level >= FULL` crossing
    /// while charging starts the fully-charged pulse.
    async fn on_battery_status_event(&mut self, event: BatteryStatusEvent) {
        if let BatteryStatus::Available { level: Some(level), .. } = event.0 {
            let was_full = self.battery.is_some_and(|b| b >= BATTERY_FULL);
            self.battery = Some(level);
            if self.charging && level >= BATTERY_FULL && !was_full {
                self.frame = 0;
            }
        }
    }

    /// Immediate charging-line edge. Kept for when RMK wires up its charging
    /// reader again; today it never fires (VBUS is read in `poll` instead).
    async fn on_charging_state_event(&mut self, event: ChargingStateEvent) {
        self.set_charging(event.charging);
    }

    /// Central view of the peripheral link.
    async fn on_peripheral_connected_event(&mut self, event: PeripheralConnectedEvent) {
        if self.role == Role::Central && event.connected != self.peer_connected {
            self.peer_connected = event.connected;
            self.frame = 0;
        }
    }

    /// Peripheral view of the central link.
    async fn on_central_connected_event(&mut self, event: CentralConnectedEvent) {
        if self.role == Role::Peripheral && event.connected != self.peer_connected {
            self.peer_connected = event.connected;
            self.frame = 0;
        }
    }

    /// Host (BLE) connection status: drives the central outer pixel's profile
    /// colour and connect / search animations. Restart the pulse/blink only on a
    /// RISING edge (start of connection or start of searching) or a profile
    /// change — not on falling edges, so a late `Connected` arriving after
    /// `LedIndicatorEvent` already marked us connected won't fire a second pulse.
    async fn on_connection_status_change_event(&mut self, event: ConnectionStatusChangeEvent) {
        let ble = event.0.ble;
        let connected = matches!(ble.state, BleState::Connected);
        let advertising = matches!(ble.state, BleState::Advertising);
        let newly_connected = connected && !self.ble_connected;
        let newly_advertising = advertising && !self.ble_advertising;
        if newly_connected || newly_advertising || ble.profile != self.ble_profile {
            self.frame = 0;
        }
        self.ble_profile = ble.profile;
        self.ble_connected = connected;
        self.ble_advertising = advertising;
    }

    /// A host LED-state (output) report can only arrive over an established host
    /// connection, so it is a reliable "connected" edge — unlike
    /// `BleState::Connected`, which some hosts (e.g. Windows) never trigger.
    /// Only fire on the first one (when not already marked connected) so toggling
    /// CapsLock later doesn't re-pulse.
    async fn on_led_indicator_event(&mut self, _event: LedIndicatorEvent) {
        if self.role == Role::Central && !self.ble_connected {
            self.ble_connected = true;
            self.frame = 0;
        }
    }

    /// Timer-driven repaint (called every `poll_interval` ms by the macro).
    async fn poll(&mut self) {
        // "Charging" = USB power present (VBUS), read straight from the POWER
        // peripheral's USBREGSTATUS register. This is what the indicator really
        // wants to show and matches how the LED behaved before: the LED tracks
        // "plugged in", not the charger IC's STAT line (which goes idle once the
        // pack is full, leaving the LED dark even while on USB). A bare register
        // read is passive — it does not touch interrupts or the regulator, so it
        // is safe alongside MPSL/nrf-sdc. `set_charging` only restarts the
        // animation on an actual edge, so re-reading every tick is jitter-free.
        let usb_present = embassy_nrf::pac::POWER.usbregstatus().read().vbusdetect();
        self.set_charging(usb_present);
        let inner = self.inner_color();
        let outer = self.outer_color();
        self.render(inner, outer).await;
        self.frame = self.frame.wrapping_add(1);
    }
}
