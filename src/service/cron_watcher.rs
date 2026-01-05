use std::sync::atomic::{AtomicI64, Ordering};

use chrono::{DateTime, Local};
use cron::Schedule;

#[derive(Debug)]
pub struct CronWatcher {
    schedule: Schedule,
    next: AtomicI64,
}

const NONE_TIMESTAMP: i64 = i64::MIN;

impl CronWatcher {
    pub fn new(schedule: &Schedule) -> Self {
        let schedule = schedule.clone();
        let next = schedule
            .upcoming(Local)
            .next()
            .map(|d| d.timestamp())
            .unwrap_or(NONE_TIMESTAMP);
        let next = AtomicI64::new(next);
        Self { schedule, next }
    }

    pub fn is_ready(&self) -> bool {
        let current = self.next.load(Ordering::SeqCst);
        if current == NONE_TIMESTAMP {
            return false;
        }

        let next_dt = DateTime::from_timestamp(current, 0).map(|dt| dt.with_timezone(&Local));

        match next_dt {
            Some(next) if chrono::Local::now() >= next => {
                let new_next = self
                    .schedule
                    .upcoming(Local)
                    .next()
                    .map(|d| d.timestamp())
                    .unwrap_or(NONE_TIMESTAMP);

                // Only succeed if no other thread beat us to it
                self.next
                    .compare_exchange(current, new_next, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            }
            _ => false,
        }
    }
}

impl PartialEq for CronWatcher {
    fn eq(&self, other: &Self) -> bool {
        self.schedule == other.schedule
            && self.next.load(Ordering::SeqCst) == other.next.load(Ordering::SeqCst)
    }
}

impl Eq for CronWatcher {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn make_schedule(expr: &str) -> Schedule {
        Schedule::from_str(expr).expect("Failed to parse cron expression")
    }

    #[test]
    fn test_new_initializes_with_next_timestamp() {
        // Every minute schedule - should always have a next occurrence
        let schedule = make_schedule("* * * * * *");
        let watcher = CronWatcher::new(&schedule);

        let next = watcher.next.load(Ordering::SeqCst);
        assert!(next != NONE_TIMESTAMP, "Expected next timestamp to be set");

        assert!(
            next >= chrono::Local::now().timestamp(),
            "Next timestamp should be in the future or now"
        );
    }

    #[test]
    fn test_next_none_timestamp() {
        let schedule = make_schedule("* * * * * *");
        let watcher = CronWatcher::new(&schedule);

        // Manually set the next timestamp to NONE_TIMESTAMP
        watcher.next.store(NONE_TIMESTAMP, Ordering::SeqCst);

        assert_eq!(
            watcher.next.load(Ordering::SeqCst),
            NONE_TIMESTAMP,
            "Expected NONE_TIMESTAMP"
        );
    }

    #[test]
    fn test_next_is_valid_timestamp() {
        let schedule = make_schedule("* * * * * *");
        let watcher = CronWatcher::new(&schedule);

        let next = watcher.next.load(Ordering::SeqCst);
        assert!(next != NONE_TIMESTAMP);

        // Verify the returned timestamp is reasonable (within the next second)
        let now = chrono::Local::now().timestamp();
        let diff = next - now;
        assert!(
            diff <= 1,
            "Next occurrence should be within 1 second for per-second schedule"
        );
    }

    #[test]
    fn test_is_ready_returns_false_when_not_yet_time() {
        // Schedule far in the future (year 2099)
        let schedule = make_schedule("0 0 0 1 1 * 2099");
        let watcher = CronWatcher::new(&schedule);

        assert!(
            !watcher.is_ready(),
            "Should not be ready when next time is in the future"
        );
    }

    #[test]
    fn test_is_ready_returns_true_and_updates_when_time_passed() {
        let schedule = make_schedule("* * * * * *");
        let watcher = CronWatcher::new(&schedule);

        // Set the next timestamp to a time in the past
        let past_timestamp = chrono::Local::now().timestamp() - 10;
        watcher.next.store(past_timestamp, Ordering::SeqCst);

        let old_next = watcher.next.load(Ordering::SeqCst);
        assert!(watcher.is_ready(), "Should be ready when time has passed");

        // Verify the next timestamp was updated
        let new_next = watcher.next.load(Ordering::SeqCst);
        assert!(
            new_next > old_next,
            "Next timestamp should be updated after is_ready returns true"
        );
    }

    #[test]
    fn test_is_ready_returns_false_when_next_is_none() {
        let schedule = make_schedule("* * * * * *");
        let watcher = CronWatcher::new(&schedule);

        // Set to NONE_TIMESTAMP
        watcher.next.store(NONE_TIMESTAMP, Ordering::SeqCst);

        assert!(
            !watcher.is_ready(),
            "Should not be ready when next timestamp is None"
        );
    }

    #[test]
    fn test_partial_eq_same_schedule_same_next() {
        let schedule = make_schedule("0 0 * * * *");
        let watcher1 = CronWatcher::new(&schedule);
        let watcher2 = CronWatcher::new(&schedule);

        // Both should have the same next timestamp since they use the same schedule
        assert_eq!(watcher1, watcher2);
    }

    #[test]
    fn test_partial_eq_same_schedule_different_next() {
        let schedule = make_schedule("0 0 * * * *");
        let watcher1 = CronWatcher::new(&schedule);
        let watcher2 = CronWatcher::new(&schedule);

        // Modify one watcher's next timestamp
        watcher2.next.store(12345, Ordering::SeqCst);

        assert_ne!(watcher1, watcher2);
    }

    #[test]
    fn test_partial_eq_different_schedule() {
        let schedule1 = make_schedule("0 0 * * * *");
        let schedule2 = make_schedule("0 30 * * * *");
        let watcher1 = CronWatcher::new(&schedule1);
        let watcher2 = CronWatcher::new(&schedule2);

        assert_ne!(watcher1, watcher2);
    }

    #[test]
    fn test_is_ready_boundary_condition_exact_time() {
        let schedule = make_schedule("* * * * * *");
        let watcher = CronWatcher::new(&schedule);

        // Set next to exactly now
        let now_timestamp = chrono::Local::now().timestamp();
        watcher.next.store(now_timestamp, Ordering::SeqCst);

        // Should be ready since now >= next
        assert!(
            watcher.is_ready(),
            "Should be ready when current time equals next time"
        );
    }
}
