use super::*;
use yue_component::{VirtualGridTrack, VirtualizerSizeUpdate};

#[test]
fn grid_auto_tracks_share_flex_space_respect_maxima_and_keep_minima() {
    let tracks = [
        VirtualGridTrack::new(px(40.), px(80.), 0.),
        VirtualGridTrack::new(px(20.), px(60.), 1.),
        VirtualGridTrack::new(px(30.), px(300.), 2.),
    ];
    assert_eq!(
        VirtualGridTrack::resolve(&tracks, px(200.), px(5.), px(10.), px(10.)).unwrap(),
        vec![px(40.), px(46.666668), px(83.333336)]
    );
    assert_eq!(
        VirtualGridTrack::resolve(&tracks, px(400.), px(0.), px(0.), px(0.)).unwrap(),
        vec![px(40.), px(60.), px(300.)]
    );
    assert_eq!(
        VirtualGridTrack::resolve(&tracks, px(10.), px(5.), px(10.), px(10.)).unwrap(),
        vec![px(40.), px(20.), px(30.)]
    );
    assert_eq!(
        VirtualGridTrack::resolve(&tracks, px(10000.), px(0.), px(0.), px(0.)).unwrap(),
        vec![px(40.), px(60.), px(300.)]
    );
    assert!(
        VirtualGridTrack::resolve(&[], px(0.), px(0.), px(0.), px(0.))
            .unwrap()
            .is_empty()
    );
    for invalid in [
        VirtualGridTrack::new(px(0.), px(20.), 1.),
        VirtualGridTrack::new(px(30.), px(20.), 1.),
        VirtualGridTrack::new(px(20.), px(30.), f32::NAN),
    ] {
        assert!(VirtualGridTrack::resolve(&[invalid], px(100.), px(0.), px(0.), px(0.)).is_err());
    }
}
#[test]
fn grid_auto_track_resize_uses_existing_known_anchor_and_equal_batch_is_noop() {
    let tracks = vec![VirtualGridTrack::new(px(20.), px(100.), 1.); 10];
    let old = VirtualGridTrack::resolve(&tracks, px(400.), px(0.), px(0.), px(0.)).unwrap();
    let mut state = VirtualizerState::new(
        Axis::Horizontal,
        source(0, 10),
        VirtualizerSizing::known(old),
    )
    .unwrap();
    let v = VirtualizerViewport::new(state.binding().unwrap(), 1, 130., px(100.), px(100.));
    state.commit(&v).unwrap();
    let next = VirtualGridTrack::resolve(&tracks, px(600.), px(0.), px(0.), px(0.)).unwrap();
    let updates: Vec<_> = next
        .iter()
        .enumerate()
        .map(|(index, extent)| VirtualizerSizeUpdate::new(index, *extent))
        .collect();
    state.update_sizes(&updates).unwrap();
    let (_, request) = state.prepare(&v, Instant::now()).unwrap();
    // key 3 remains 10px into the viewport: 3*60+10.
    assert_eq!(request.as_ref().unwrap().offset(), 190.);
    state.confirm(request.as_ref().unwrap(), 190., &v).unwrap();
    let snapshot = state.snapshot(&v.clone().with_offset(190.)).unwrap();
    state.update_sizes(&updates).unwrap();
    assert!(state.validate_snapshot(&snapshot).is_ok());
}
