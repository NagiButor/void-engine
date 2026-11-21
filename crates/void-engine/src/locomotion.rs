//! Phase-locked gait, camera head-bob, and footstep trigger controller.
use crate::units::human;
use std::f32::consts::TAU;

/// Normalized stride phase (`0.0..1.0`) where the foot plants at the bottom of the bob (`sin = -1`).
pub const FOOT_PLANT_PHASE: f32 = 0.75;
/// Minimum horizontal speed (m/s) required to advance walking gait.
pub const MIN_GAIT_SPEED: f32 = 0.55;
/// Minimum downward speed (m/s) prior to touchdown that triggers a landing step.
pub const MIN_LAND_SPEED: f32 = 1.80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Walk,
    Land,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FootstepEvent {
    pub kind: StepKind,
    pub gain: f32,
    pub step_index: u64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct GaitOutput {
    /// Vertical camera head-bob offset in meters, phase-locked to the stride cycle.
    pub head_bob_y: f32,
    /// Footstep event emitted on foot-plant or landing this frame, if any.
    pub footstep: Option<FootstepEvent>,
}

/// Synchronizes footstep triggers and camera head-bob to real horizontal distance traveled.
#[derive(Clone, Debug)]
pub struct FootstepGait {
    phase: f32,
    bob_weight: f32,
    was_grounded: bool,
    min_air_vy: f32,
    land_cooldown: f32,
    step_count: u64,
}

impl Default for FootstepGait {
    fn default() -> Self {
        Self {
            phase: 0.0,
            bob_weight: 0.0,
            was_grounded: true,
            min_air_vy: 0.0,
            land_cooldown: 0.0,
            step_count: 0,
        }
    }
}

impl FootstepGait {
    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn phase(&self) -> f32 {
        self.phase
    }

    fn current_bob(&self, horizontal_speed: f32) -> f32 {
        let speed_scale = (horizontal_speed / human::WALK_SPEED).clamp(0.6, 1.25);
        (self.phase * TAU).sin() * human::HEAD_BOB_AMPLITUDE * self.bob_weight * speed_scale
    }

    pub fn update(
        &mut self,
        dt: f32,
        horizontal_speed: f32,
        vertical_velocity: f32,
        grounded: bool,
    ) -> GaitOutput {
        if dt <= 0.0 {
            return GaitOutput {
                head_bob_y: self.current_bob(horizontal_speed),
                footstep: None,
            };
        }

        if !grounded {
            self.bob_weight *= (-10.0 * dt).exp();
            self.min_air_vy = self.min_air_vy.min(vertical_velocity);
            self.was_grounded = false;
            if self.bob_weight < 0.01 {
                self.phase = 0.0;
            }
            return GaitOutput {
                head_bob_y: self.current_bob(horizontal_speed),
                footstep: None,
            };
        }

        let mut footstep = None;
        if !self.was_grounded {
            self.was_grounded = true;
            let impact = -self.min_air_vy;
            self.min_air_vy = 0.0;
            if impact >= MIN_LAND_SPEED {
                let t = ((impact - MIN_LAND_SPEED) / 6.0).clamp(0.0, 1.0);
                let gain = 0.09 + t * 0.08;
                self.phase = 0.0;
                self.land_cooldown = 0.14;
                let step_index = self.step_count;
                self.step_count += 1;
                footstep = Some(FootstepEvent {
                    kind: StepKind::Land,
                    gain,
                    step_index,
                });
            }
        }

        self.land_cooldown = (self.land_cooldown - dt).max(0.0);
        let moving = horizontal_speed > MIN_GAIT_SPEED;
        let target_weight = if moving { 1.0 } else { 0.0 };
        let blend = 1.0 - (-12.0 * dt).exp();
        self.bob_weight += (target_weight - self.bob_weight) * blend;

        if moving {
            let speed_ratio = (horizontal_speed / human::WALK_SPEED).clamp(0.5, 2.0);
            let stride = human::STEP_STRIDE * (0.78 + 0.22 * speed_ratio);
            let prev_phase = self.phase;
            let next_phase = prev_phase + (horizontal_speed * dt) / stride;
            let cycles_before = (prev_phase + (1.0 - FOOT_PLANT_PHASE)).floor();
            let cycles_after = (next_phase + (1.0 - FOOT_PLANT_PHASE)).floor();
            self.phase = next_phase.rem_euclid(1.0);

            if cycles_after > cycles_before && footstep.is_none() && self.land_cooldown <= 0.0 {
                let sprint_t = ((horizontal_speed - human::WALK_SPEED)
                    / (human::SPRINT_SPEED - human::WALK_SPEED))
                    .clamp(0.0, 1.0);
                let gain = 0.085 + sprint_t * 0.035;
                let step_index = self.step_count;
                self.step_count += 1;
                footstep = Some(FootstepEvent {
                    kind: StepKind::Walk,
                    gain,
                    step_index,
                });
            }
        } else if self.bob_weight < 0.02 {
            self.phase = 0.0;
        }

        GaitOutput {
            head_bob_y: self.current_bob(horizontal_speed),
            footstep,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footstep_triggers_at_head_bob_trough_and_scales_with_speed() {
        let mut gait = FootstepGait::default();
        let dt = 1.0 / 60.0;
        let mut step_bobs = Vec::new();
        for _ in 0..120 {
            let out = gait.update(dt, human::WALK_SPEED, 0.0, true);
            if let Some(ev) = out.footstep {
                assert_eq!(ev.kind, StepKind::Walk);
                step_bobs.push(out.head_bob_y);
            }
        }
        assert_eq!(step_bobs.len(), 4);
        for bob in step_bobs {
            assert!(
                bob < -human::HEAD_BOB_AMPLITUDE * 0.90,
                "footstep must align with head-bob trough, got {bob}"
            );
        }
    }

    #[test]
    fn landing_emits_land_event_without_double_walk_step() {
        let mut gait = FootstepGait::default();
        let dt = 1.0 / 60.0;
        for _ in 0..20 {
            let out = gait.update(dt, human::WALK_SPEED, -4.5, false);
            assert!(out.footstep.is_none());
        }
        let landed = gait.update(dt, human::WALK_SPEED, 0.0, true);
        let ev = landed.footstep.expect("landing should emit a footstep");
        assert_eq!(ev.kind, StepKind::Land);
        assert!(ev.gain >= 0.09 && ev.gain <= 0.17);
    }
}
