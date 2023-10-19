mod detection;
mod gamepad;

pub use detection::GamepadDetector;
pub use gamepad::Gamepad;
pub use gamepad::{Button, DpadAxis, GamepadEvent, Stick, StickAxis, Trigger};
