use super::{AnyGamepad, AnyGamepadEvent, Stick, StickAxis, Trigger};
use crate::locomotion::LocomotionCommand;
use std::error::Error;

pub struct GamepadInputInterpreter {
    gamepad: AnyGamepad,
    state: GamepadState,
}

impl GamepadInputInterpreter {
    pub fn new() -> Result<GamepadInputInterpreter, Box<dyn Error>> {
        Ok(GamepadInputInterpreter {
            gamepad: AnyGamepad::new()?,
            state: GamepadState::new(),
        })
    }

    pub fn process_input(&mut self) -> Result<LocomotionCommand, Box<dyn Error>> {
        self.gamepad.read_events(|event| {
            match event {
                AnyGamepadEvent::StickAdjusted(stick, axis, value) => {
                    if stick == Stick::Left && axis == StickAxis::Horizontal {
                        self.state.left_stick_horizontal = value;
                    };
                }

                AnyGamepadEvent::TriggerAdjusted(trigger, value) => {
                    match trigger {
                        Trigger::Left => {
                            self.state.left_trigger = value;
                        }
                        Trigger::Right => {
                            self.state.right_trigger = value;
                        }
                    };
                }

                AnyGamepadEvent::Disconnected => {
                    self.state = GamepadState::new();
                }

                _ => (),
            };
        })?;

        Ok(LocomotionCommand::new(
            self.state.right_trigger - self.state.left_trigger,
            self.state.left_stick_horizontal,
        ))
    }
}

struct GamepadState {
    right_trigger: f64,
    left_trigger: f64,
    left_stick_horizontal: f64,
}

impl GamepadState {
    fn new() -> Self {
        Self {
            right_trigger: 0.0,
            left_trigger: 0.0,
            left_stick_horizontal: 0.0,
        }
    }
}
