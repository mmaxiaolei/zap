use super::*;

#[test]
fn test_format_sigfigs() {
    assert_eq!(format_sigfigs(0.000456, 2,), "0.00046");
    assert_eq!(format_sigfigs(0.043256, 3,), "0.0433");
    assert_eq!(format_sigfigs(0.01, 2,), "0.010");
    assert_eq!(format_sigfigs(10., 3,), "10.0");
    assert_eq!(format_sigfigs(456.719, 4,), "456.7");
    assert_eq!(format_sigfigs(10., 2,), "10");
}

#[test]
fn test_human_readable_precise_duration() {
    assert_eq!(
        human_readable_precise_duration(Duration::milliseconds(3)),
        "3 ms".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration(Duration::milliseconds(10)),
        "10 ms".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration(Duration::milliseconds(3141)),
        "3.14 sec".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration(Duration::milliseconds(19961)),
        "20.0 sec".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration(Duration::seconds(61)),
        "1.02 min".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration(Duration::minutes(930)),
        "15.5 hours".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration(Duration::hours(46)),
        "1.92 days".to_owned()
    );
    assert_eq!(
        human_readable_precise_duration(Duration::weeks(2)),
        ">1 week".to_owned()
    );
}

#[test]
fn test_human_readable_approx_duration() {
    assert_eq!(
        human_readable_approx_duration(Duration::milliseconds(2), false),
        "just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::seconds(2), false),
        "just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::milliseconds(2), true),
        "Just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::seconds(2), true),
        "Just now".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::seconds(90), false),
        "1 min ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::minutes(100), false),
        "1 hour ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::minutes(130), false),
        "2 hours ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::days(4), false),
        "4 days ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::weeks(1), false),
        "1 week ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::weeks(15), false),
        "3 months ago".to_owned()
    );
    assert_eq!(
        human_readable_approx_duration(Duration::weeks(520), false),
        "9 years ago".to_owned()
    );
}

#[test]
fn format_short_time_ago_matches_orca() {
    assert_eq!(format_short_time_ago_secs(0), "now");
    assert_eq!(format_short_time_ago_secs(10), "now");
    assert_eq!(format_short_time_ago_secs(59), "now");
    assert_eq!(format_short_time_ago_secs(60), "1m");
    assert_eq!(format_short_time_ago_secs(5 * 60), "5m");
    assert_eq!(format_short_time_ago_secs(59 * 60), "59m");
    assert_eq!(format_short_time_ago_secs(60 * 60), "1h");
    assert_eq!(format_short_time_ago_secs(20 * 60 * 60), "20h");
    assert_eq!(format_short_time_ago_secs(24 * 60 * 60), "1d");
    assert_eq!(format_short_time_ago_secs(2 * 24 * 60 * 60), "2d");
}

#[test]
fn secs_until_next_short_time_ago_change_boundaries() {
    assert_eq!(secs_until_next_short_time_ago_change(0), 60);
    assert_eq!(secs_until_next_short_time_ago_change(10), 50);
    assert_eq!(secs_until_next_short_time_ago_change(59), 1);
    // Exact minute boundary: label is already Nm, next flip in 60s.
    assert_eq!(secs_until_next_short_time_ago_change(60), 60);
    assert_eq!(secs_until_next_short_time_ago_change(61), 59);
    assert_eq!(secs_until_next_short_time_ago_change(5 * 60 + 15), 45);
    assert_eq!(secs_until_next_short_time_ago_change(59 * 60), 60);
    // 1h exact → next hour boundary in 3600s (1h → 2h).
    assert_eq!(secs_until_next_short_time_ago_change(3600), 3600);
    assert_eq!(secs_until_next_short_time_ago_change(3600 + 100), 3500);
    assert_eq!(secs_until_next_short_time_ago_change(20 * 3600 + 1), 3599);
    assert_eq!(secs_until_next_short_time_ago_change(86400), 86400);
    assert_eq!(secs_until_next_short_time_ago_change(86400 + 10), 86390);
}
