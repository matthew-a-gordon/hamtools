use std::path::PathBuf;

use chrono::{DateTime, Duration, TimeZone, Utc};
use sat_tui::cache::{Cache, Dataset, Decision, State, decide, format_age, tles_stale};

fn t(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 26, h, m, 0).unwrap()
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sat-tui-cache-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

mod store {
    use super::*;

    #[test]
    fn what_is_stored_is_loaded_back_with_its_fetch_time() {
        let cache = Cache::new(scratch("roundtrip"));

        cache
            .store(Dataset::Tles, "ISS\n1 25544U...\n", t(9, 30))
            .unwrap();

        let loaded = cache.load(Dataset::Tles).unwrap().unwrap();
        assert_eq!(loaded.body, "ISS\n1 25544U...\n");
        assert_eq!(loaded.fetched_at, t(9, 30));
    }

    #[test]
    fn nothing_cached_loads_as_none() {
        let cache = Cache::new(scratch("empty"));

        assert!(cache.load(Dataset::Transmitters).unwrap().is_none());
    }

    #[test]
    fn the_two_datasets_do_not_collide_and_storing_again_replaces() {
        let cache = Cache::new(scratch("separate"));
        cache.store(Dataset::Tles, "tles-v1", t(8, 0)).unwrap();
        cache.store(Dataset::Transmitters, "[]", t(8, 5)).unwrap();

        cache.store(Dataset::Tles, "tles-v2", t(9, 0)).unwrap();

        assert_eq!(cache.load(Dataset::Tles).unwrap().unwrap().body, "tles-v2");
        assert_eq!(
            cache.load(Dataset::Transmitters).unwrap().unwrap().body,
            "[]"
        );
    }

    #[test]
    fn a_failed_attempt_is_remembered_without_touching_the_cached_data() {
        let cache = Cache::new(scratch("attempt"));
        cache.store(Dataset::Tles, "good", t(6, 0)).unwrap();

        cache.record_attempt(Dataset::Tles, t(9, 0)).unwrap();

        let state = cache.state(Dataset::Tles);
        assert_eq!(state.fetched_at, Some(t(6, 0)));
        assert_eq!(state.last_attempt, Some(t(9, 0)));
        assert_eq!(cache.load(Dataset::Tles).unwrap().unwrap().body, "good");
    }

    #[test]
    fn a_missing_cache_has_an_empty_state() {
        let cache = Cache::new(scratch("nostate"));

        assert_eq!(cache.state(Dataset::Tles), State::default());
    }
}

mod policy {
    use super::*;

    fn state(fetched: Option<DateTime<Utc>>, attempt: Option<DateTime<Utc>>) -> State {
        State {
            fetched_at: fetched,
            last_attempt: attempt,
        }
    }

    fn hours_ago(h: i64) -> DateTime<Utc> {
        t(12, 0) - Duration::hours(h)
    }

    #[test]
    fn data_never_fetched_is_fetched() {
        assert_eq!(
            decide(Dataset::Tles, &State::default(), t(12, 0), false),
            Decision::Fetch
        );
        assert_eq!(
            decide(Dataset::Transmitters, &State::default(), t(12, 0), false),
            Decision::Fetch
        );
    }

    #[test]
    fn automatic_refresh_waits_until_the_data_is_over_a_day_old() {
        let fresh = state(Some(hours_ago(23)), Some(hours_ago(23)));
        let old = state(Some(hours_ago(25)), Some(hours_ago(25)));

        assert_eq!(
            decide(Dataset::Tles, &fresh, t(12, 0), false),
            Decision::Fresh
        );
        assert_eq!(
            decide(Dataset::Tles, &old, t(12, 0), false),
            Decision::Fetch
        );
        assert_eq!(
            decide(Dataset::Transmitters, &fresh, t(12, 0), false),
            Decision::Fresh
        );
        assert_eq!(
            decide(Dataset::Transmitters, &old, t(12, 0), false),
            Decision::Fetch
        );
    }

    #[test]
    fn celestrak_is_not_asked_again_within_two_hours_even_after_a_failure() {
        // Old data, but we tried 30 minutes ago and it failed: wait out the other 90 minutes.
        let s = state(Some(hours_ago(30)), Some(t(12, 0) - Duration::minutes(30)));

        assert_eq!(
            decide(Dataset::Tles, &s, t(12, 0), false),
            Decision::TooSoon {
                retry_in: Duration::minutes(90)
            }
        );
    }

    #[test]
    fn a_manual_refresh_skips_the_day_rule_but_not_the_two_hour_rule_for_tles() {
        let recent = state(Some(hours_ago(1)), Some(hours_ago(1)));
        let earlier = state(Some(hours_ago(3)), Some(hours_ago(3)));

        assert_eq!(
            decide(Dataset::Tles, &recent, t(12, 0), true),
            Decision::TooSoon {
                retry_in: Duration::hours(1)
            }
        );
        assert_eq!(
            decide(Dataset::Tles, &earlier, t(12, 0), true),
            Decision::Fetch
        );
    }

    #[test]
    fn transmitter_data_has_no_minimum_interval() {
        let recent = state(Some(hours_ago(1)), Some(hours_ago(1)));

        assert_eq!(
            decide(Dataset::Transmitters, &recent, t(12, 0), true),
            Decision::Fetch
        );
    }

    #[test]
    fn tles_older_than_a_week_are_stale() {
        let now = t(12, 0);

        assert!(tles_stale(
            now - Duration::days(7) - Duration::seconds(1),
            now
        ));
        assert!(!tles_stale(
            now - Duration::days(6) - Duration::hours(23),
            now
        ));
    }
}

#[test]
fn ages_are_shown_in_the_largest_whole_unit() {
    assert_eq!(format_age(Duration::seconds(45)), "<1m");
    assert_eq!(format_age(Duration::minutes(5)), "5m");
    assert_eq!(format_age(Duration::minutes(90)), "1h");
    assert_eq!(format_age(Duration::hours(47)), "47h");
    assert_eq!(format_age(Duration::hours(49)), "2d");
    assert_eq!(format_age(Duration::days(7) + Duration::hours(3)), "7d");
}
