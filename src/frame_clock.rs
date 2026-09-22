//! Display-paced wakeups. Drawing and playback remain on the window thread.

use std::{
    io,
    sync::{Arc, Condvar, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, IDXGIOutput};
use windows_sys::Win32::{
    Foundation::HWND,
    Graphics::Gdi::{MonitorFromWindow, MONITOR_DEFAULTTONEAREST},
    UI::WindowsAndMessaging::{PostMessageW, WM_APP},
};

pub const FRAME_MESSAGE: u32 = WM_APP + 3;
const FALLBACK_INTERVAL: Duration = Duration::from_nanos(16_666_667);
const OUTPUT_RETRY: Duration = Duration::from_secs(1);
const MIN_FRAME_INTERVAL: Duration = Duration::from_millis(1);

#[derive(Default)]
struct State {
    running: bool,
    pending: bool,
    stopped: bool,
    run_revision: u64,
    output_revision: u64,
}

#[derive(Default)]
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

impl Shared {
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|error| error.into_inner())
    }
}

pub struct FrameClock {
    shared: Arc<Shared>,
}

impl FrameClock {
    /// Creates a paused clock. The owner must drop it before destroying HWND.
    pub fn new(hwnd: HWND) -> io::Result<Self> {
        if hwnd.is_null() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Null frame-clock window",
            ));
        }
        let shared = Arc::new(Shared::default());
        let worker_shared = Arc::clone(&shared);
        // No pointer or application state is dereferenced on the worker.
        let window = hwnd as usize;
        let worker = thread::Builder::new()
            .name("teleprompt-frame-clock".into())
            .spawn(move || run(window, worker_shared))?;
        // WaitForVBlank has no cancellation API. Arc-owned state lets a driver-
        // blocked worker finish safely without ever blocking UI shutdown.
        drop(worker);
        Ok(Self { shared })
    }

    pub fn set_running(&self, running: bool) {
        let mut state = self.shared.lock();
        if state.running != running {
            state.running = running;
            state.run_revision = state.run_revision.wrapping_add(1);
        }
        self.shared.wake.notify_one();
    }

    /// Call after painting the consumed frame message, or when discarding that
    /// message because playback is paused/hidden. Unrelated paints do not ack it.
    pub fn acknowledge(&self) {
        let mut state = self.shared.lock();
        state.pending = false;
        self.shared.wake.notify_one();
    }

    pub fn invalidate_output(&self) {
        let mut state = self.shared.lock();
        state.output_revision = state.output_revision.wrapping_add(1);
        self.shared.wake.notify_one();
    }
}

impl Drop for FrameClock {
    fn drop(&mut self) {
        let mut state = self.shared.lock();
        state.stopped = true;
        state.running = false;
        // Posting holds this same mutex, so no new message can be posted once
        // Drop returns. An already queued message carries no borrowed pointers.
        self.shared.wake.notify_one();
    }
}

fn find_output(monitor: usize) -> Option<IDXGIOutput> {
    if monitor == 0 {
        return None;
    }
    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        let mut adapter_index = 0;
        while let Ok(adapter) = factory.EnumAdapters1(adapter_index) {
            let mut output_index = 0;
            while let Ok(output) = adapter.EnumOutputs(output_index) {
                if let Ok(desc) = output.GetDesc() {
                    if desc.AttachedToDesktop.as_bool() && desc.Monitor.0 as usize == monitor {
                        return Some(output);
                    }
                }
                output_index += 1;
            }
            adapter_index += 1;
        }
    }
    None
}

fn current(state: &State, run_revision: u64, output_revision: u64) -> bool {
    !state.stopped
        && state.running
        && !state.pending
        && state.run_revision == run_revision
        && state.output_revision == output_revision
}

fn run(window: usize, shared: Arc<Shared>) {
    let hwnd = window as HWND;
    let mut output: Option<IDXGIOutput> = None;
    let mut output_monitor = 0;
    let mut output_revision_seen = u64::MAX;
    let mut retry_at = Instant::now();
    let mut last_frame: Option<Instant> = None;

    loop {
        let (run_revision, output_revision) = {
            let mut state = shared.lock();
            while !state.stopped && (!state.running || state.pending) {
                state = shared
                    .wake
                    .wait(state)
                    .unwrap_or_else(|error| error.into_inner());
            }
            if state.stopped {
                return;
            }
            (state.run_revision, state.output_revision)
        };

        let monitor = unsafe { MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST) } as usize;
        let now = Instant::now();
        if monitor != output_monitor || output_revision != output_revision_seen {
            output = None;
            output_monitor = monitor;
            output_revision_seen = output_revision;
            retry_at = now;
        }
        if output.is_none() && now >= retry_at {
            output = find_output(monitor);
            retry_at = Instant::now() + OUTPUT_RETRY;
        }

        let wait_started = Instant::now();
        let paced = output
            .as_ref()
            .is_some_and(|display| unsafe { display.WaitForVBlank().is_ok() });
        let after_wait = Instant::now();
        // Check the whole frame interval, not only time inside WaitForVBlank:
        // a real vblank can legitimately arrive just after a slow paint ends.
        let too_fast = paced
            && last_frame
                .is_some_and(|previous| after_wait.duration_since(previous) < MIN_FRAME_INTERVAL);
        let fallback = !paced || too_fast;
        if output.is_some() && fallback {
            output = None;
            retry_at = after_wait + OUTPUT_RETRY;
        }
        let mut state = shared.lock();
        if state.stopped {
            return;
        }
        if !current(&state, run_revision, output_revision) {
            continue;
        }
        if fallback {
            // Deadline scheduling avoids adding paint duration to every fallback
            // interval. Missed frames are discarded, never queued for replay.
            let deadline = last_frame.unwrap_or(wait_started) + FALLBACK_INTERVAL;
            while current(&state, run_revision, output_revision) && Instant::now() < deadline {
                let remaining = deadline.saturating_duration_since(Instant::now());
                let (next, _) = shared
                    .wake
                    .wait_timeout(state, remaining)
                    .unwrap_or_else(|error| error.into_inner());
                state = next;
            }
        }
        if !current(&state, run_revision, output_revision) {
            continue;
        }
        state.pending = true;
        if unsafe { PostMessageW(hwnd, FRAME_MESSAGE, 0, 0) } != 0 {
            last_frame = Some(Instant::now());
        } else {
            state.pending = false;
            // A temporarily full queue must not turn the worker into a busy loop.
            drop(
                shared
                    .wake
                    .wait_timeout(state, FALLBACK_INTERVAL)
                    .unwrap_or_else(|error| error.into_inner()),
            );
        }
    }
}
