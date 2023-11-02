use std::ffi::CString;
use std::io::Error as IoError;
use std::mem;
use std::mem::MaybeUninit;
use std::os::fd::AsRawFd;
use std::os::fd::FromRawFd;
use std::os::fd::OwnedFd;
use std::os::unix::prelude::OsStrExt;
use std::path::Path;

// 💁‍♂️ At present, this is hard-wired to support an Xbox controller via Bluetooth using the xpadneo driver.
// No attempt has been made to deal with different values and/or events that might be reported by different
// controllers.

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum GamepadEvent {
    ButtonPressed(Button),
    StickAdjusted(Stick, StickAxis, f64),
    TriggerAdjusted(Trigger, f64),
    DpadAdjusted(DpadAxis, f64),
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Stick {
    Left,
    Right,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum StickAxis {
    Vertical,
    Horizontal,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Trigger {
    Left,
    Right,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DpadAxis {
    Vertical,
    Horizontal,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum Button {
    A,
    B,
    X,
    Y,
    TL,
    TR,
    SELECT,
    START,
    MODE,
    THUMBL,
    THUMBR,
}

const DEADZONE_THRESHOLD: f64 = 0.15;

pub struct Gamepad {
    device_fd: OwnedFd,
    recovering_from_dropped: bool,
}

impl Gamepad {
    pub fn new(device_file_path: &Path) -> Result<Gamepad, IoError> {
        let device_fd = open_gamepad_device(&device_file_path)?;

        let gamepad = Gamepad {
            device_fd,
            recovering_from_dropped: false,
        };

        Ok(gamepad)
    }

    pub fn read_events(
        &mut self,
        mut handler: impl FnMut(GamepadEvent) -> (),
    ) -> std::io::Result<()> {
        // The kernel caches input events in an internal buffer until they are read via the device file
        // descriptor. If events are not read fast enough, the internal buffer can fill up. If there is no space
        // left to store an incoming event, the kernel will:
        // - discard the entire contents of the buffer,
        // - queue a SYN_DROPPED event to let userspace know that events are missing,
        // - queue the incoming event.

        // From experimentation:
        // - The size of the internal buffer depends on various factors, but it holds about 256 events on the test
        // setup for this project.
        // - When continuously manipulating a controller, the largest possible time interval between reads without
        // getting SYN_DROPPED events seems to be around 300 milliseconds (assuming reads of up to 256 events).

        const NUMBER_OF_EVENTS_IN_BUFFER: usize = 256;
        const INPUT_EVENT_SIZE: usize = mem::size_of::<libc::input_event>();

        let mut buffer = [MaybeUninit::<libc::input_event>::uninit(); NUMBER_OF_EVENTS_IN_BUFFER];

        let bytes_read = unsafe {
            libc::read(
                self.device_fd.as_raw_fd(),
                buffer.as_mut_ptr() as *mut libc::c_void,
                NUMBER_OF_EVENTS_IN_BUFFER * INPUT_EVENT_SIZE,
            )
        };

        if bytes_read < 0 {
            let error = std::io::Error::last_os_error();

            if error
                .raw_os_error()
                .is_some_and(|code| code == libc::EAGAIN)
            {
                return Ok(());
            }

            return Err(error);
        }

        let bytes_read = bytes_read as usize;

        assert!(bytes_read % INPUT_EVENT_SIZE == 0);
        let events_read: usize = bytes_read / INPUT_EVENT_SIZE;

        for event in &buffer[0..events_read] {
            let event = unsafe { event.assume_init() };

            if self.recovering_from_dropped {
                if event.type_ == EV_SYN && event.code == SYN_REPORT {
                    self.recovering_from_dropped = false;

                    // The correct response at this point is to re-sync with the current state of the device.

                    // However, the assumption is that for present purposes an operator would notice when a
                    // controller becomes unresponsive and would manipulate triggers and sticks to send new
                    // events until a controlled system behaves as expected again.

                    // Let's see whether this assumption holds.
                }
            } else {
                if event.type_ == EV_SYN && event.code == SYN_DROPPED {
                    log::error!("Gamepad event buffer overflow. Events may have been dropped.");
                    self.recovering_from_dropped = true;
                } else {
                    // Multiple input events may be grouped together into "packets of input data changes occurring
                    // at the same moment in time". Each group of one or more input events is therefore followed
                    // by a SYN_REPORT event that marks the end of the "packet".

                    // This grouping is ignored here: each individual input event is dispatched immediately (This
                    // matches the behaviour of SDL.).

                    let event = match event.type_ {
                        EV_KEY => process_key_event(event.code, event.value),
                        EV_ABS => process_absolute_event(event.code, event.value),
                        _ => None,
                    };

                    if let Some(event) = event {
                        handler(event);
                    }
                }
            }
        }

        Ok(())
    }
}

// Event types of interest.
const EV_SYN: libc::__u16 = 0x00;
const EV_KEY: libc::__u16 = 0x01;
const EV_ABS: libc::__u16 = 0x03;

// EV_SYN event codes of interest.
const SYN_REPORT: libc::__u16 = 0;
const SYN_DROPPED: libc::__u16 = 3;

// EV_KEY event codes of interest.
const BTN_A: libc::__u16 = 0x130;
const BTN_B: libc::__u16 = 0x131;
const BTN_X: libc::__u16 = 0x133;
const BTN_Y: libc::__u16 = 0x134;
const BTN_TL: libc::__u16 = 0x136;
const BTN_TR: libc::__u16 = 0x137;
const BTN_SELECT: libc::__u16 = 0x13a;
const BTN_START: libc::__u16 = 0x13b;
const BTN_MODE: libc::__u16 = 0x13c;
const BTN_THUMBL: libc::__u16 = 0x13d;
const BTN_THUMBR: libc::__u16 = 0x13e;

// EV_ABS event codes of interest.
const ABS_X: libc::__u16 = 0x00;
const ABS_Y: libc::__u16 = 0x01;
const ABS_Z: libc::__u16 = 0x02;
const ABS_RX: libc::__u16 = 0x03;
const ABS_RY: libc::__u16 = 0x04;
const ABS_RZ: libc::__u16 = 0x05;
const ABS_HAT0X: libc::__u16 = 0x10;
const ABS_HAT0Y: libc::__u16 = 0x11;

fn process_key_event(code: libc::__u16, value: libc::__s32) -> Option<GamepadEvent> {
    // For now an event will be raised immediately on key down.
    if value != 1 {
        return None;
    }

    match code {
        BTN_A => Some(GamepadEvent::ButtonPressed(Button::A)),
        BTN_B => Some(GamepadEvent::ButtonPressed(Button::B)),
        BTN_X => Some(GamepadEvent::ButtonPressed(Button::X)),
        BTN_Y => Some(GamepadEvent::ButtonPressed(Button::Y)),
        BTN_TL => Some(GamepadEvent::ButtonPressed(Button::TL)),
        BTN_TR => Some(GamepadEvent::ButtonPressed(Button::TR)),
        BTN_SELECT => Some(GamepadEvent::ButtonPressed(Button::SELECT)),
        BTN_START => Some(GamepadEvent::ButtonPressed(Button::START)),
        BTN_MODE => Some(GamepadEvent::ButtonPressed(Button::MODE)),
        BTN_THUMBL => Some(GamepadEvent::ButtonPressed(Button::THUMBL)),
        BTN_THUMBR => Some(GamepadEvent::ButtonPressed(Button::THUMBR)),
        _ => None,
    }
}

fn process_absolute_event(code: libc::__u16, value: libc::__s32) -> Option<GamepadEvent> {
    match code {
        ABS_X => Some(create_stick_event(
            Stick::Left,
            StickAxis::Horizontal,
            value,
        )),
        ABS_Y => Some(create_stick_event(Stick::Left, StickAxis::Vertical, value)),
        ABS_RX => Some(create_stick_event(
            Stick::Right,
            StickAxis::Horizontal,
            value,
        )),
        ABS_RY => Some(create_stick_event(Stick::Right, StickAxis::Vertical, value)),

        ABS_Z => Some(create_trigger_event(Trigger::Left, value)),
        ABS_RZ => Some(create_trigger_event(Trigger::Right, value)),

        ABS_HAT0X => Some(create_dpad_event(DpadAxis::Horizontal, value)),
        ABS_HAT0Y => Some(create_dpad_event(DpadAxis::Vertical, value)),

        _ => None,
    }
}

fn create_stick_event(stick: Stick, axis: StickAxis, value: libc::__s32) -> GamepadEvent {
    // `value` is expected to be in the range [-32768, 32767].
    let value = if value <= -32768 {
        -1.0
    } else if value >= 32767 {
        1.0
    } else {
        let value = if value < 0 {
            value as f64 / 32768.0
        } else {
            value as f64 / 32767.0
        };

        apply_deadzone(value)
    };

    GamepadEvent::StickAdjusted(stick, axis, value)
}

fn create_trigger_event(trigger: Trigger, value: libc::__s32) -> GamepadEvent {
    // `value` is expected to be in the range [0, 1023].
    let value = if value <= 0 {
        0.0
    } else if value >= 1023 {
        1.0
    } else {
        apply_deadzone(value as f64 / 1023.0)
    };

    GamepadEvent::TriggerAdjusted(trigger, value)
}

fn create_dpad_event(axis: DpadAxis, value: libc::__s32) -> GamepadEvent {
    // `value` is expected to be -1, 0 or 1.
    let value = if value <= -1 {
        -1.0
    } else if value >= 1 {
        1.0
    } else {
        0.0
    };

    GamepadEvent::DpadAdjusted(axis, value)
}

// Even just moving around the controller will cause the sticks to wobble and register events. Using and then
// releasing the triggers will also not land them perfectly on the all zero mark. Values below a small threshold
// are therefore ignored.
fn apply_deadzone(value: f64) -> f64 {
    if value.abs() < DEADZONE_THRESHOLD {
        0.0
    } else {
        value
    }
}

fn open_gamepad_device(device_file_path: &Path) -> Result<OwnedFd, IoError> {
    let device_file_path = CString::new(device_file_path.as_os_str().as_bytes()).unwrap();

    let fd = unsafe {
        libc::open(
            device_file_path.as_ptr(),
            libc::O_RDONLY | libc::O_NONBLOCK | libc::O_CLOEXEC,
        )
    };

    if fd == -1 {
        Err(IoError::last_os_error())
    } else {
        Ok(unsafe { OwnedFd::from_raw_fd(fd) })
    }
}
