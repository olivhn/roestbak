use libc;
use std::error::Error;
use std::io::Error as IoError;
use std::mem::MaybeUninit;
use std::ptr;
use std::time::Duration;

pub enum IterationOutcome {
    Conclude,
    KeepGoing,
}

pub fn start_runloop(
    interval: Duration,
    mut block: impl FnMut() -> Result<IterationOutcome, Box<dyn Error>>,
) -> Result<(), Box<dyn Error>> {
    let mut start_of_upcoming_iteration = now();

    loop {
        match block()? {
            IterationOutcome::Conclude => {
                return Ok(());
            }

            IterationOutcome::KeepGoing => {
                // The new deadline for starting the next iteration is `interval` added to the previous deadline. This should result
                // in a regular, non-drifting schedule.
                start_of_upcoming_iteration += interval;

                // Should an iteration take longer than `interval`, the next iteration will start immediately.
                //
                // Note that such an overrun could take longer than just one interval. Maintaining the original schedule could therefore
                // lead to a number of iterations running back-to-back until `start_of_upcoming_iteration` catches up to present time.
                // This is not the desired behaviour, so `start_of_upcoming_iteration` is reset to present time in this case. A new regular
                // schedule can then (hopefully) start from this point onward.
                let end_of_current_iteration = now();
                if end_of_current_iteration > start_of_upcoming_iteration {
                    let overrun_duration = end_of_current_iteration - start_of_upcoming_iteration;
                    log::warn!(
                        "Runloop iteration overrun. Allotted time: {:?}, overran by: {:?}.",
                        interval,
                        overrun_duration
                    );

                    start_of_upcoming_iteration = end_of_current_iteration;
                } else {
                    sleep_until(start_of_upcoming_iteration);
                }
            }
        }
    }
}

// Rust internally represents `libc::timespec` values using a private `Timespec` type, which includes operations for arithmetic, comparing
// and so on. As a point in time is—in present context—defined as a duration since some agreed upon past moment, the publicly available
// `Duration` type is used(/abused?) for this purpose here. This avoids needlessly duplicating the logic for some needed operations.

// ⚠️ Contrary to the `Duration` type, `libc::timespec`'s fields are signed. A negative value for `tv_sec` could be used to represent a
// point in time before epoch. We therefore need to ensure that the clock we use won't emit negative values. This should not be a problem
// for the monotonic clock. From clock_gettime(3):
//
// > A nonsettable system-wide clock that represents monotonic time since—as described by POSIX—"some unspecified point in the past".  On
// > Linux, that point corresponds to the number of seconds that the system has been running since it was booted.
const CLOCK: libc::clockid_t = libc::CLOCK_MONOTONIC;

fn now() -> Duration {
    let mut timespec: MaybeUninit<libc::timespec> = MaybeUninit::uninit();

    let result = unsafe { libc::clock_gettime(CLOCK, timespec.as_mut_ptr()) };
    if result != 0 {
        let error = IoError::last_os_error();
        panic!(
            "Retrieving time from clock is expected to succeed. Error: {}",
            error
        );
    }

    let timespec = unsafe { timespec.assume_init() };

    Duration::new(
        u64::try_from(timespec.tv_sec).expect("timespec.tv_sec out of bounds."),
        u32::try_from(timespec.tv_nsec).expect("timespec.tv_nsec out of bounds."),
    )
}

fn sleep_until(deadline: Duration) {
    let deadline = libc::timespec {
        tv_sec: libc::time_t::try_from(deadline.as_secs())
            .expect("deadline.as_secs() out of bounds."),
        tv_nsec: libc::c_long::try_from(deadline.subsec_nanos())
            .expect("deadline.subsec_nanos() out of bounds."),
    };

    let result =
        unsafe { libc::clock_nanosleep(CLOCK, libc::TIMER_ABSTIME, &deadline, ptr::null_mut()) };

    // This implementation assumes that signals are blocked so that this call will never be interrupted.
    if result != 0 {
        panic!(
            "Sleep is expected to succeed (are signals blocked?). Error code: {}",
            result
        );
    }
}
