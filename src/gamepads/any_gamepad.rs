use super::{Button, DpadAxis, Gamepad, GamepadDetector, GamepadEvent, Stick, StickAxis, Trigger};
use std::error::Error;

#[derive(Debug, Copy, Clone)]
pub enum AnyGamepadEvent {
    ButtonPressed(Button),
    StickAdjusted(Stick, StickAxis, f64),
    TriggerAdjusted(Trigger, f64),
    DpadAdjusted(DpadAxis, f64),
    Disconnected,
}

pub struct AnyGamepad {
    detector: GamepadDetector,
    current_gamepad: Option<Gamepad>,
}

impl AnyGamepad {
    pub fn new() -> Result<AnyGamepad, Box<dyn Error>> {
        let detector = GamepadDetector::new()?;

        Ok(AnyGamepad {
            detector,
            current_gamepad: None,
        })
    }

    pub fn read_events(
        &mut self,
        mut handler: impl FnMut(AnyGamepadEvent) -> (),
    ) -> Result<(), Box<dyn Error>> {
        self.detector.process_updates()?;

        if self.current_gamepad.is_none() {
            if let Some(gamepad_device_file_path) = self.detector.next_gamepad_device() {
                match Gamepad::new(&gamepad_device_file_path) {
                    Ok(gamepad) => {
                        log::info!("Using gamepad at {}", gamepad_device_file_path.display());
                        self.current_gamepad = Some(gamepad);
                    }
                    Err(error) => {
                        log::warn!("Could not open gamepad at {} (udev might still be fixing permissions). - Cause: {}", gamepad_device_file_path.display(), error);
                    }
                };
            }
        }

        if let Some(ref mut gamepad) = self.current_gamepad {
            let gamepad_handler = |gamepad_event: GamepadEvent| {
                handler(gamepad_event.into());
            };

            match gamepad.read_events(gamepad_handler) {
                Ok(_) => (),
                Err(error) => {
                    log::warn!("Closing gamepad due to read error (this could be an intentional disconnect). - Cause: {}", error);
                    self.current_gamepad = None;
                    handler(AnyGamepadEvent::Disconnected);
                }
            };
        }

        Ok(())
    }
}

impl From<GamepadEvent> for AnyGamepadEvent {
    fn from(gamepad_event: GamepadEvent) -> Self {
        match gamepad_event {
            GamepadEvent::ButtonPressed(button) => AnyGamepadEvent::ButtonPressed(button),
            GamepadEvent::StickAdjusted(stick, axis, value) => {
                AnyGamepadEvent::StickAdjusted(stick, axis, value)
            }
            GamepadEvent::TriggerAdjusted(trigger, value) => {
                AnyGamepadEvent::TriggerAdjusted(trigger, value)
            }
            GamepadEvent::DpadAdjusted(axis, value) => AnyGamepadEvent::DpadAdjusted(axis, value),
        }
    }
}
