use rdev::{listen, simulate, Button, Event, EventType, Key, SimulateError};
use serial_test::serial;
use std::error::Error;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

// Modern replacement for lazy_static using std::sync::OnceLock
static EVENT_CHANNEL: OnceLock<(Mutex<Sender<Event>>, Mutex<Receiver<Event>>)> = OnceLock::new();

fn get_channel() -> &'static (Mutex<Sender<Event>>, Mutex<Receiver<Event>>) {
    EVENT_CHANNEL.get_or_init(|| {
        let (send, recv) = channel();
        (Mutex::new(send), Mutex::new(recv))
    })
}

fn send_event(event: Event) {
    if let Ok(sender) = get_channel().0.lock() {
        // Ignore errors if the receiver dropped
        let _ = sender.send(event);
    }
}

/// Drains any stray background events (e.g. slight mouse jitter) currently in the queue.
fn drain_stray_events(recv: &Receiver<Event>) {
    while recv.try_recv().is_ok() {}
}

/// Simulates events and verifies they are intercepted by the listener.
/// Ignores unrelated OS noise by waiting specifically for the expected event until timeout.
fn sim_then_listen(
    events: impl IntoIterator<Item = EventType>,
    timeout_per_event: Duration,
) -> Result<(), Box<dyn Error>> {
    // Ensure background listener thread is active once
    static LISTENER_SPAWNED: OnceLock<()> = OnceLock::new();
    LISTENER_SPAWNED.get_or_init(|| {
        thread::spawn(|| {
            if let Err(e) = listen(send_event) {
                eprintln!("rdev listener error: {e:?}");
            }
        });
        // Give the OS hook a brief window to bind
        thread::sleep(Duration::from_millis(150));
    });

    let channel = get_channel();
    let recv = channel.1.lock().map_err(|e| e.to_string())?;

    // Discard any residual system events before beginning
    drain_stray_events(&recv);

    for expected in events {
        simulate(&expected)?;

        // Filter through incoming events until we see the simulated one or time out
        let start = Instant::now();
        let mut matched = false;

        while start.elapsed() < timeout_per_event {
            let remaining = timeout_per_event.saturating_sub(start.elapsed());
            match recv.recv_timeout(remaining) {
                Ok(received_event) if received_event.event_type == expected => {
                    matched = true;
                    break;
                }
                Ok(_stray_event) => {
                    // Stray OS event (mouse jitter, key release, etc.) - keep waiting
                    continue;
                }
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => {
                    return Err("Event channel disconnected unexpectedly".into());
                }
            }
        }

        assert!(
            matched,
            "Timed out waiting for simulated event: {expected:?}"
        );
    }

    Ok(())
}

#[test]
#[serial]
fn test_listen_and_simulate() -> Result<(), Box<dyn Error>> {
    // Settle pending user input from running cargo test
    thread::sleep(Duration::from_millis(100));

    let discrete_events = vec![
        EventType::KeyPress(Key::KeyS),
        EventType::KeyRelease(Key::KeyS),
        EventType::ButtonPress(Button::Right),
        EventType::ButtonRelease(Button::Right),
        EventType::Wheel { delta_x: 0, delta_y: 1 },
        EventType::Wheel { delta_x: 0, delta_y: -1 },
    ];

    // Sample mouse movement across coordinates
    let move_events = (0..50).map(|pixel| EventType::MouseMove {
        x: (pixel * 10) as f64,
        y: (pixel * 10) as f64,
    });

    let events: Vec<EventType> = discrete_events.into_iter().chain(move_events).collect();

    sim_then_listen(events, Duration::from_millis(300))?;
    Ok(())
}
