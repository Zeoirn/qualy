use rdev::{grab, listen, simulate, Event, EventType, Key};
use serial_test::serial;
use std::error::Error;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

// Replaces lazy_static with std::sync::OnceLock
static EVENT_CHANNEL: OnceLock<(Mutex<Sender<Event>>, Mutex<Receiver<Event>>)> = OnceLock::new();
static GRAB_ACTIVE: AtomicBool = AtomicBool::new(false);

fn get_channel() -> &'static (Mutex<Sender<Event>>, Mutex<Receiver<Event>>) {
    EVENT_CHANNEL.get_or_init(|| {
        let (send, recv) = channel();
        (Mutex::new(send), Mutex::new(recv))
    })
}

fn send_event(event: Event) {
    if let Ok(sender) = get_channel().0.lock() {
        let _ = sender.send(event);
    }
}

/// Filter callback for `rdev::grab`: drops Tab keys only when GRAB_ACTIVE is true
fn filter_tab_events(event: Event) -> Option<Event> {
    if GRAB_ACTIVE.load(Ordering::Acquire) {
        match event.event_type {
            EventType::KeyPress(Key::Tab) | EventType::KeyRelease(Key::Tab) => {
                // Intercept and swallow Tab
                return None;
            }
            _ => {}
        }
    }
    Some(event)
}

/// Drains any residual OS noise (e.g. mouse micro-movements, Enter key release)
fn drain_events(recv: &Receiver<Event>) {
    while recv.try_recv().is_ok() {}
}

/// Waits for a specific event type while ignoring unrelated background OS noise
fn wait_for_event(
    recv: &Receiver<Event>,
    expected: &EventType,
    timeout: Duration,
) -> Result<bool, Box<dyn Error>> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let remaining = timeout.saturating_sub(start.elapsed());
        match recv.recv_timeout(remaining) {
            Ok(ev) if ev.event_type == *expected => return Ok(true),
            Ok(_stray) => continue, // Ignore cursor drift or other background noise
            Err(RecvTimeoutError::Timeout) => return Ok(false),
            Err(RecvTimeoutError::Disconnected) => {
                return Err("Event channel disconnected".into());
            }
        }
    }
    Ok(false)
}

/// Asserts that a forbidden event does not arrive within the given window
fn assert_no_event(
    recv: &Receiver<Event>,
    forbidden: &[EventType],
    window: Duration,
) {
    let start = Instant::now();
    while start.elapsed() < window {
        let remaining = window.saturating_sub(start.elapsed());
        if let Ok(ev) = recv.recv_timeout(remaining) {
            if forbidden.contains(&ev.event_type) {
                panic!("Received swallowed event: {:?}", ev.event_type);
            }
        }
    }
}

#[test]
#[serial]
fn test_grab() -> Result<(), Box<dyn Error>> {
    // Settle terminal input (Enter key release)
    thread::sleep(Duration::from_millis(150));

    // Initialize the background listener and grab hooks once
    static HOOKS_INITIALIZED: OnceLock<()> = OnceLock::new();
    HOOKS_INITIALIZED.get_or_init(|| {
        // Start listener to forward non-grabbed events into the channel
        thread::spawn(|| {
            if let Err(e) = listen(send_event) {
                eprintln!("Listener failed: {e:?}");
            }
        });

        // Start grab filter to intercept/swallow events
        thread::spawn(|| {
            if let Err(e) = grab(filter_tab_events) {
                eprintln!("Grab failed: {e:?}");
            }
        });

        // Brief delay to let the OS register the event taps
        thread::sleep(Duration::from_millis(250));
    });

    let recv = get_channel().1.lock().map_err(|e| e.to_string())?;
    drain_events(&recv);

    // Enable grabbing for this test
    GRAB_ACTIVE.store(true, Ordering::Release);

    let event_press = EventType::KeyPress(Key::KeyS);
    let event_release = EventType::KeyRelease(Key::KeyS);

    // 1. Verify normal events pass through to listen
    simulate(&event_press)?;
    assert!(
        wait_for_event(&recv, &event_press, Duration::from_millis(500))?,
        "Expected KeyS press was not received"
    );

    simulate(&event_release)?;
    assert!(
        wait_for_event(&recv, &event_release, Duration::from_millis(500))?,
        "Expected KeyS release was not received"
    );

    // 2. Verify Tab events are swallowed by grab and never reach listen
    drain_events(&recv);

    simulate(&EventType::KeyPress(Key::Tab))?;
    simulate(&EventType::KeyRelease(Key::Tab))?;

    assert_no_event(
        &recv,
        &[EventType::KeyPress(Key::Tab), EventType::KeyRelease(Key::Tab)],
        Duration::from_millis(300),
    );

    // Clean up flag for any subsequent tests
    GRAB_ACTIVE.store(false, Ordering::Release);

    Ok(())
}
