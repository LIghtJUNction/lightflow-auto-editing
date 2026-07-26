use super::audit::timeline_duration;
use super::*;

#[test]
fn derives_duration_from_contiguous_timeline_segments() {
    let edl = json!({"video_segments":[
        {"timeline_in":0.0,"timeline_out":14.34},
        {"timeline_in":14.34,"timeline_out":33.195}
    ]});
    assert_eq!(timeline_duration(&edl).expect("derived duration"), 33.195);
}

#[test]
fn rejects_discontinuous_legacy_timeline() {
    let edl = json!({"video_segments":[
        {"timeline_in":0.0,"timeline_out":3.0},
        {"timeline_in":3.1,"timeline_out":6.0}
    ]});
    assert_eq!(
        timeline_duration(&edl).expect_err("gap must reject"),
        "edl timeline segments are discontinuous"
    );
}
