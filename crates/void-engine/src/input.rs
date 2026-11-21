//! Gamepad input abstraction (axes, radial deadzone, response curve, trigger hysteresis, rumble).
use gilrs::{
    ff::{BaseEffect, BaseEffectType, Effect, EffectBuilder, Repeat, Replay},
    Axis, Button, EventType, GamepadId, Gilrs,
};
use glam::Vec2;
use std::time::{Duration, Instant};

const TRIGGER_PRESS_THRESHOLD: f32 = 0.35;
const TRIGGER_RELEASE_THRESHOLD: f32 = 0.18;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub enum GamepadButton {
    South = 0,
    East = 1,
    West = 2,
    North = 3,
    LeftBumper = 4,
    RightBumper = 5,
    LeftThumb = 6,
    RightThumb = 7,
    Start = 8,
    Select = 9,
    DPadUp = 10,
    DPadDown = 11,
    DPadLeft = 12,
    DPadRight = 13,
}

impl GamepadButton {
    pub const COUNT: usize = 14;
    pub const ALL: [Self; Self::COUNT] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftBumper,
        Self::RightBumper,
        Self::LeftThumb,
        Self::RightThumb,
        Self::Start,
        Self::Select,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
    ];

    const fn to_gilrs(self) -> Button {
        match self {
            Self::South => Button::South,
            Self::East => Button::East,
            Self::West => Button::West,
            Self::North => Button::North,
            Self::LeftBumper => Button::LeftTrigger,
            Self::RightBumper => Button::RightTrigger,
            Self::LeftThumb => Button::LeftThumb,
            Self::RightThumb => Button::RightThumb,
            Self::Start => Button::Start,
            Self::Select => Button::Select,
            Self::DPadUp => Button::DPadUp,
            Self::DPadDown => Button::DPadDown,
            Self::DPadLeft => Button::DPadLeft,
            Self::DPadRight => Button::DPadRight,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct GamepadState {
    pub connected: bool,
    pub name: Option<String>,
    pub move_axes: Vec2,
    pub look_axes: Vec2,
    pub left_trigger: f32,
    pub right_trigger: f32,
    pub left_trigger_down: bool,
    pub left_trigger_pressed: bool,
    pub right_trigger_down: bool,
    pub right_trigger_pressed: bool,
    buttons_down: [bool; GamepadButton::COUNT],
    buttons_pressed: [bool; GamepadButton::COUNT],
}

impl GamepadState {
    pub fn down(&self, btn: GamepadButton) -> bool {
        self.buttons_down[btn as usize]
    }

    pub fn pressed(&self, btn: GamepadButton) -> bool {
        self.buttons_pressed[btn as usize]
    }
}

pub struct GamepadInput {
    gilrs: Option<Gilrs>,
    active: Option<GamepadId>,
    prev_buttons: [bool; GamepadButton::COUNT],
    prev_lt: bool,
    prev_rt: bool,
    active_effect: Option<Effect>,
    rumble_until: Option<Instant>,
}

impl Drop for GamepadInput {
    fn drop(&mut self) {
        self.stop_rumble();
    }
}

impl Default for GamepadInput {
    fn default() -> Self {
        Self::new()
    }
}

impl GamepadInput {
    pub fn new() -> Self {
        let gilrs = match Gilrs::new() {
            Ok(g) => Some(g),
            Err(e) => {
                eprintln!("Gamepad init skipped: {e}");
                None
            }
        };
        let active = gilrs.as_ref().and_then(|g| {
            g.gamepads()
                .find(|(_, gp)| gp.is_connected())
                .map(|(id, _)| id)
        });
        Self {
            gilrs,
            active,
            prev_buttons: [false; GamepadButton::COUNT],
            prev_lt: false,
            prev_rt: false,
            active_effect: None,
            rumble_until: None,
        }
    }

    pub fn stop_rumble(&mut self) {
        if let Some(prev) = self.active_effect.take() {
            let _ = prev.stop();
        }
        self.rumble_until = None;
    }

    pub fn poll(&mut self, deadzone: f32) -> GamepadState {
        if self
            .rumble_until
            .is_some_and(|until| Instant::now() >= until)
        {
            self.stop_rumble();
        }
        let mut state = GamepadState::default();
        let Some(gilrs) = self.gilrs.as_mut() else {
            return state;
        };

        while let Some(ev) = gilrs.next_event() {
            match ev.event {
                EventType::Connected => {
                    if self.active.is_none() {
                        self.active = Some(ev.id);
                    }
                }
                EventType::Disconnected => {
                    if self.active == Some(ev.id) {
                        self.active = gilrs
                            .gamepads()
                            .find(|(id, gp)| *id != ev.id && gp.is_connected())
                            .map(|(id, _)| id);
                        self.prev_buttons = [false; GamepadButton::COUNT];
                        self.prev_lt = false;
                        self.prev_rt = false;
                    }
                }
                EventType::ButtonPressed(_, _) | EventType::AxisChanged(_, _, _)
                    if self.active != Some(ev.id) =>
                {
                    self.active = Some(ev.id);
                    self.prev_buttons = [false; GamepadButton::COUNT];
                    self.prev_lt = false;
                    self.prev_rt = false;
                }
                _ => {}
            }
        }

        let Some(id) = self.active else {
            return state;
        };
        let gp = gilrs.gamepad(id);
        if !gp.is_connected() {
            self.active = None;
            return state;
        }

        state.connected = true;
        state.name = Some(gp.name().to_owned());

        let dz = deadzone.clamp(0.02, 0.40);
        let raw_move = Vec2::new(gp.value(Axis::LeftStickX), gp.value(Axis::LeftStickY));
        let raw_look = Vec2::new(gp.value(Axis::RightStickX), gp.value(Axis::RightStickY));
        state.move_axes = apply_radial_deadzone(raw_move, dz, false);
        state.look_axes = apply_radial_deadzone(raw_look, dz, true);

        let lt_axis = gp.value(Axis::LeftZ).clamp(0.0, 1.0);
        let lt_btn = gp
            .button_data(Button::LeftTrigger2)
            .map(|d| d.value())
            .unwrap_or_else(|| {
                if gp.is_pressed(Button::LeftTrigger2) {
                    1.0
                } else {
                    0.0
                }
            });
        state.left_trigger = lt_axis.max(lt_btn).clamp(0.0, 1.0);

        let rt_axis = gp.value(Axis::RightZ).clamp(0.0, 1.0);
        let rt_btn = gp
            .button_data(Button::RightTrigger2)
            .map(|d| d.value())
            .unwrap_or_else(|| {
                if gp.is_pressed(Button::RightTrigger2) {
                    1.0
                } else {
                    0.0
                }
            });
        state.right_trigger = rt_axis.max(rt_btn).clamp(0.0, 1.0);

        state.left_trigger_down = trigger_hysteresis(state.left_trigger, self.prev_lt);
        state.left_trigger_pressed = state.left_trigger_down && !self.prev_lt;
        self.prev_lt = state.left_trigger_down;

        state.right_trigger_down = trigger_hysteresis(state.right_trigger, self.prev_rt);
        state.right_trigger_pressed = state.right_trigger_down && !self.prev_rt;
        self.prev_rt = state.right_trigger_down;

        for btn in GamepadButton::ALL {
            let idx = btn as usize;
            let is_down = gp.is_pressed(btn.to_gilrs());
            state.buttons_down[idx] = is_down;
            state.buttons_pressed[idx] = is_down && !self.prev_buttons[idx];
            self.prev_buttons[idx] = is_down;
        }

        state
    }

    pub fn rumble(&mut self, strong: f32, weak: f32, duration_ms: u32, enabled: bool) {
        if !enabled {
            return;
        }
        self.stop_rumble();
        let (Some(gilrs), Some(id)) = (self.gilrs.as_mut(), self.active) else {
            return;
        };
        if !gilrs.gamepad(id).is_ff_supported() {
            return;
        }
        let dur = gilrs::ff::Ticks::from_ms(duration_ms.clamp(25, 1000));
        let strong_mag = (strong.clamp(0.0, 1.0) * u16::MAX as f32) as u16;
        let weak_mag = (weak.clamp(0.0, 1.0) * u16::MAX as f32) as u16;
        let effect = EffectBuilder::new()
            .add_effect(BaseEffect {
                kind: BaseEffectType::Strong {
                    magnitude: strong_mag,
                },
                scheduling: Replay {
                    play_for: dur,
                    ..Default::default()
                },
                ..Default::default()
            })
            .add_effect(BaseEffect {
                kind: BaseEffectType::Weak {
                    magnitude: weak_mag,
                },
                scheduling: Replay {
                    play_for: dur,
                    ..Default::default()
                },
                ..Default::default()
            })
            .repeat(Repeat::For(dur))
            .gamepads(&[id])
            .finish(gilrs);
        if let Ok(eff) = effect {
            if eff.play().is_ok() {
                self.active_effect = Some(eff);
                self.rumble_until = Some(
                    Instant::now() + Duration::from_millis(duration_ms.clamp(25, 1000) as u64),
                );
            }
        }
    }
}

/// Applies a radial deadzone and optional cubic-blended aim response curve.
pub fn apply_radial_deadzone(raw: Vec2, deadzone: f32, aim_curve: bool) -> Vec2 {
    let mag = raw.length();
    if mag <= deadzone {
        return Vec2::ZERO;
    }
    let norm = ((mag - deadzone) / (1.0 - deadzone).max(0.001)).clamp(0.0, 1.0);
    let curved = if aim_curve {
        norm * (0.35 + 0.65 * norm * norm)
    } else {
        norm
    };
    (raw / mag) * curved
}

/// Hysteresis for analog triggers (hair-trigger feel without bounce).
pub fn trigger_hysteresis(value: f32, prev_down: bool) -> bool {
    if prev_down {
        value >= TRIGGER_RELEASE_THRESHOLD
    } else {
        value >= TRIGGER_PRESS_THRESHOLD
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn radial_deadzone_zeros_drift_and_preserves_direction() {
        assert_eq!(
            apply_radial_deadzone(Vec2::new(0.05, -0.06), 0.12, true),
            Vec2::ZERO
        );
        let full = apply_radial_deadzone(Vec2::new(1.0, 0.0), 0.12, true);
        assert!((full.x - 1.0).abs() < 1e-4 && full.y.abs() < 1e-4);
        let half = apply_radial_deadzone(Vec2::new(0.56, 0.0), 0.12, true);
        assert!(half.x > 0.0 && half.x < 0.5);
    }

    #[test]
    fn trigger_hysteresis_avoids_chatter() {
        assert!(!trigger_hysteresis(0.25, false));
        assert!(trigger_hysteresis(0.40, false));
        assert!(trigger_hysteresis(0.25, true));
        assert!(!trigger_hysteresis(0.10, true));
    }
}
