use super::*;
use crate::renderer::TextStyle;

fn shape(x: f32) -> Draw {
    Draw::Shape(Rect::new(x, 300., 100., 100.), 8., 0., [1., 1., 1., 1.])
}
fn wallpaper() -> Draw {
    Draw::Image(
        "wallpapers/dunes.jpg".into(),
        Rect::new(0., 0., 1920., 1080.),
        0.,
        false,
    )
}
fn ring(x: f32) -> Draw {
    Draw::Shape(
        Rect::new(x - 4., 296., 108., 108.),
        12.,
        2.,
        [1., 1., 1., 1.],
    )
}
fn area(damage: Damage) -> Rect {
    damage.expect("partial damage")
}

#[test]
fn the_first_frame_and_unknown_buffer_ages_redraw_everything() {
    let mut tracker = Tracker::default();
    tracker.push(&[wallpaper(), shape(100.)]);
    assert!(tracker.damage(1).is_none());
    tracker.push(&[wallpaper(), shape(100.)]);
    assert!(tracker.damage(0).is_none());
    assert!(
        tracker.damage(3).is_none(),
        "older than the recorded history"
    );
}

#[test]
fn an_unchanged_scene_has_no_damage() {
    let mut tracker = Tracker::default();
    tracker.push(&[wallpaper(), shape(100.)]);
    tracker.push(&[wallpaper(), shape(100.)]);
    let damage = area(tracker.damage(1));
    assert!(damage.w <= 0. || damage.h <= 0.);
}

#[test]
fn a_moved_shape_damages_its_old_and_new_place_only() {
    let mut tracker = Tracker::default();
    tracker.push(&[wallpaper(), shape(100.)]);
    tracker.push(&[wallpaper(), shape(400.)]);
    let damage = area(tracker.damage(1));
    assert_eq!((damage.x, damage.w), (99., 402.));
    assert_eq!((damage.y, damage.h), (299., 102.));
}

#[test]
fn an_inserted_focus_ring_realigns_instead_of_redrawing_everything() {
    let mut tracker = Tracker::default();
    tracker.push(&[wallpaper(), shape(100.), shape(400.), shape(700.)]);
    tracker.push(&[
        wallpaper(),
        shape(100.),
        ring(400.),
        shape(400.),
        shape(700.),
    ]);
    let damage = area(tracker.damage(1));
    assert_eq!((damage.x, damage.w), (395., 110.));
    tracker.push(&[wallpaper(), shape(100.), shape(400.), shape(700.)]);
    let removed = area(tracker.damage(1));
    assert_eq!((removed.x, removed.w), (395., 110.));
}

#[test]
fn older_back_buffers_need_every_change_since_their_frame() {
    let mut tracker = Tracker::default();
    tracker.push(&[wallpaper(), shape(100.)]);
    tracker.push(&[wallpaper(), shape(400.)]);
    tracker.push(&[wallpaper(), shape(400.), shape(1200.)]);
    let latest = area(tracker.damage(1));
    assert_eq!(latest.x, 1199.);
    let two = area(tracker.damage(2));
    assert_eq!((two.x, two.x + two.w), (99., 1301.));
}

#[test]
fn changed_clip_or_fade_state_redraws_everything() {
    let mut tracker = Tracker::default();
    let clip = |y: f32| Draw::Clip(Some(Rect::new(0., y, 1920., 500.)));
    tracker.push(&[wallpaper(), clip(100.), shape(100.), Draw::Clip(None)]);
    tracker.push(&[wallpaper(), clip(120.), shape(100.), Draw::Clip(None)]);
    assert!(tracker.damage(1).is_none());
    tracker.push(&[
        wallpaper(),
        clip(120.),
        Draw::EdgeFade(Some([0., 1., 2., 3.])),
        shape(100.),
    ]);
    assert!(tracker.damage(1).is_none());
}

#[test]
fn text_damages_the_full_width_of_its_line() {
    let mut tracker = Tracker::default();
    let style = TextStyle {
        size: 20.,
        weight: 400,
        tracking: 0.,
        line_height: 1.2,
        align_right: false,
    };
    let clock = |value: &str| {
        Draw::Text(
            value.into(),
            Rect::new(80., 40., 120., 30.),
            style,
            [1.; 4],
            false,
        )
    };
    tracker.push(&[wallpaper(), clock("18:30")]);
    tracker.push(&[wallpaper(), clock("18:31")]);
    let damage = area(tracker.damage(1));
    assert_eq!((damage.x, damage.w), (0., 1920.));
    assert_eq!((damage.y, damage.h), (20., 70.));
}

#[test]
fn shadows_include_their_blur() {
    let mut tracker = Tracker::default();
    let shadow =
        |a: f32| Draw::Shadow(Rect::new(500., 500., 200., 100.), 12., 18., [0., 0., 0., a]);
    tracker.push(&[wallpaper(), shadow(0.2)]);
    tracker.push(&[wallpaper(), shadow(0.4)]);
    let damage = area(tracker.damage(1));
    assert_eq!((damage.x, damage.w), (481., 238.));
}

#[test]
fn culling_covers_every_pixel_the_damage_clears() {
    use crate::renderer::Renderer;
    let contains = |outer: Rect, inner: Rect| {
        outer.x <= inner.x + 1e-3
            && outer.y <= inner.y + 1e-3
            && outer.x + outer.w >= inner.x + inner.w - 1e-3
            && outer.y + outer.h >= inner.y + inner.h - 1e-3
    };
    let damage = Rect::new(100., 300., 3., 3.);
    for (width, height) in [
        (640, 360),
        (1280, 720),
        (1366, 768),
        (1920, 1080),
        (1920, 1200),
    ] {
        let pixels = Renderer::damage_pixels(damage, width, height);
        assert!(contains(
            Renderer::damage_area(pixels, width, height),
            damage
        ));
    }
    // At 640x360 a drawable pixel spans 3 logical units: the cleared pixels reach x 99..105,
    // so a command at x 104 must still be drawn although it misses the logical damage.
    let pixels = Renderer::damage_pixels(damage, 640, 360);
    let neighbour = bounds(&Draw::Shape(
        Rect::new(104.2, 300., 0.5, 3.),
        0.,
        0.,
        [1., 1., 1., 1.],
    ))
    .expect("shape bounds");
    assert!(!intersects(damage, neighbour));
    assert!(intersects(
        Renderer::damage_area(pixels, 640, 360),
        neighbour
    ));
}

#[test]
fn an_unchanged_frame_names_a_pixel_inside_the_drawable() {
    use crate::renderer::Renderer;
    // EGL_KHR_partial_update is told this rect and the frame is scissored to it. A rect
    // outside the surface leaves the driver nothing to preserve: Mali presented black.
    for (width, height) in [(1920, 1080), (1280, 720), (3840, 2160), (1920, 1200)] {
        let [x, y, w, h] = Renderer::damage_pixels(Rect::default(), width, height);
        assert!(w >= 1 && h >= 1, "{width}x{height}: empty damage {w}x{h}");
        assert!(
            x >= 0 && y >= 0 && x + w <= width as i32 && y + h <= height as i32,
            "{width}x{height}: [{x}, {y}, {w}, {h}] lies outside the drawable"
        );
    }
}
