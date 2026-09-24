//! Repeats the commands of held signals (game file `repeat`).
//!
//! Some games send a signal as `1` while the trigger is held and `0` when it is released,
//! instead of one pulse per shot. For signals with `repeat`, the engine sends the commands on
//! press and then every `repeat` ms until the signal is released.
//!
//! This module only keeps the schedule; the engine sends the commands. It uses
//! `std::time::Instant` and takes the current time as parameter, so it can be tested
//! without a runtime.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// What a received value changed for a repeating signal entry.
#[derive(Debug, PartialEq, Eq)]
pub enum Change {
    /// The signal was pressed: send the commands now, repeats are scheduled.
    Pressed,
    /// The signal is still held: only the value for the next repeats was updated.
    StillHeld,
    /// The signal was released: repeats stopped.
    Released,
    /// `0` while not held: nothing to do.
    NotHeld,
}

struct Held {
    /// Value of the signal, passed to the repeated commands as `{VALUE}`.
    value: String,
    interval: Duration,
    next: Instant,
}

/// Held repeating signal entries, by index in `GameConfig::signals`.
#[derive(Default)]
pub struct Repeater {
    held: HashMap<usize, Held>,
}

/// Returns true if `value` means "held": anything except `0` and empty.
fn is_held(value: &str) -> bool {
    let value = value.trim();
    !value.is_empty() && value != "0"
}

impl Repeater {
    /// Signal entry `idx` (repeating every `interval`) received `value` at `now`.
    pub fn update(&mut self, idx: usize, interval: Duration, value: &str, now: Instant) -> Change {
        if !is_held(value) {
            return match self.held.remove(&idx) {
                Some(_) => Change::Released,
                None => Change::NotHeld,
            };
        }
        match self.held.get_mut(&idx) {
            Some(held) => {
                held.value = value.to_string();
                Change::StillHeld
            }
            None => {
                self.held.insert(
                    idx,
                    Held {
                        value: value.to_string(),
                        interval,
                        next: now + interval,
                    },
                );
                Change::Pressed
            }
        }
    }

    /// Stops all repeats (game end).
    pub fn clear(&mut self) {
        self.held.clear();
    }

    /// When the next repeat is due, or `None` if nothing is held.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.held.values().map(|held| held.next).min()
    }

    /// Returns the entries due at `now` with their value, and schedules their next repeat.
    /// Repeats missed because the engine was busy are skipped, not sent in a burst.
    pub fn take_due(&mut self, now: Instant) -> Vec<(usize, String)> {
        let mut due = Vec::new();
        for (&idx, held) in &mut self.held {
            if held.next > now {
                continue;
            }
            due.push((idx, held.value.clone()));
            held.next += held.interval;
            if held.next <= now {
                held.next = now + held.interval;
            }
        }
        due.sort_unstable_by_key(|(idx, _)| *idx);
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const INTERVAL: Duration = Duration::from_millis(100);

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn repeats_while_held_until_released() {
        let start = Instant::now();
        let mut repeater = Repeater::default();
        assert_eq!(repeater.next_deadline(), None);

        assert_eq!(repeater.update(3, INTERVAL, "1", start), Change::Pressed);
        assert_eq!(repeater.next_deadline(), Some(start + ms(100)));
        assert!(repeater.take_due(start + ms(99)).is_empty());
        assert_eq!(repeater.take_due(start + ms(100)), [(3, "1".to_string())]);
        // The schedule does not drift when a repeat is sent late.
        assert_eq!(repeater.take_due(start + ms(210)), [(3, "1".to_string())]);
        assert_eq!(repeater.next_deadline(), Some(start + ms(300)));

        assert_eq!(
            repeater.update(3, INTERVAL, "0", start + ms(250)),
            Change::Released
        );
        assert_eq!(repeater.next_deadline(), None);
        assert_eq!(
            repeater.update(3, INTERVAL, "0", start + ms(260)),
            Change::NotHeld
        );
    }

    #[test]
    fn value_while_held_only_updates_the_value() {
        let start = Instant::now();
        let mut repeater = Repeater::default();
        repeater.update(0, INTERVAL, "1", start);
        assert_eq!(
            repeater.update(0, INTERVAL, "255", start + ms(50)),
            Change::StillHeld
        );
        // Not rescheduled by the second value.
        assert_eq!(repeater.take_due(start + ms(100)), [(0, "255".to_string())]);
        assert_eq!(
            repeater.update(0, INTERVAL, "", start + ms(150)),
            Change::Released
        );
    }

    #[test]
    fn skips_missed_repeats() {
        let start = Instant::now();
        let mut repeater = Repeater::default();
        repeater.update(0, INTERVAL, "1", start);
        assert_eq!(repeater.take_due(start + ms(450)).len(), 1);
        assert_eq!(repeater.next_deadline(), Some(start + ms(550)));
    }

    #[test]
    fn clear_stops_all_repeats() {
        let start = Instant::now();
        let mut repeater = Repeater::default();
        repeater.update(0, INTERVAL, "1", start);
        repeater.update(1, ms(50), "1", start);
        assert_eq!(repeater.next_deadline(), Some(start + ms(50)));
        repeater.clear();
        assert_eq!(repeater.next_deadline(), None);
        assert!(repeater.take_due(start + ms(1000)).is_empty());
    }
}
