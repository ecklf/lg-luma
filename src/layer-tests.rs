use super::*;

#[test]
fn layers_sample_only_the_fitted_viewport() {
    assert_eq!(layer_uv(Size::HD), [0., 1., 1., 0.]);
    assert_eq!(layer_uv(Size::UHD), [0., 1., 1., 0.]);
    assert_eq!(layer_uv(Size::new(1280, 800)), [0., 0.95, 1., 0.05]);
    assert_eq!(layer_uv(Size::new(2560, 1080)), [0.125, 1., 0.875, 0.]);
}

#[test]
fn layer_sampling_preserves_direct_pixel_positions_across_resizes() {
    for size in [
        Size::HD,
        Size::new(1280, 800),
        Size::new(2560, 1080),
        Size::new(800, 1280),
        Size::new(1281, 801),
        Size::new(2561, 1081),
        Size::UHD,
    ] {
        let viewport = Viewport::new(size);
        let [left, top, right, bottom] = layer_uv(size);
        // A page must occupy the same pixels while fading and once drawn directly again.
        // Include both axes, the edges, and positions inside the page/subpage content.
        for x in [0., 0.1, 0.5, 0.9, 1.] {
            for y in [0., 0.1, 0.5, 0.9, 1.] {
                let direct_x = viewport.x as f32 + x * viewport.width as f32;
                let direct_y = viewport.y as f32 + (1. - y) * viewport.height as f32;
                let sampled_x = (left + x * (right - left)) * size.width as f32;
                let sampled_y = (top + y * (bottom - top)) * size.height as f32;
                assert!((sampled_x - direct_x).abs() < 0.001, "{size:?}: x={x}");
                assert!((sampled_y - direct_y).abs() < 0.001, "{size:?}: y={y}");
            }
        }
    }
}

/// Run after `pnpm assets`, with an OpenGL display (e.g. `xvfb-run -a cargo test
/// --package luma layer_pixels_match_direct_rendering -- --ignored --test-threads=1`).
#[test]
#[ignore = "requires an OpenGL display and generated font assets"]
fn layer_pixels_match_direct_rendering() {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    video
        .gl_attr()
        .set_context_profile(sdl2::video::GLProfile::Compatibility);
    video.gl_attr().set_context_version(2, 1);
    let mut window = video
        .window("Layer viewport regression", 1280, 800)
        .opengl()
        .hidden()
        .build()
        .unwrap();
    let _context = window.gl_create_context().unwrap();
    gl::load_with(|name| video.gl_get_proc_address(name) as *const _);
    let mut renderer = Renderer::new(Path::new(".build/assets/font-400.ttf"), false, 0.).unwrap();
    let background = Draw::Shape(Rect::new(0., 0., 1920., 1080.), 0., 0., [0., 0., 0., 1.]);
    let rect = Rect::new(300., 100., 300., 100.);
    let direct = [
        background.clone(),
        Draw::Clip(Some(Rect::new(320., 110., 260., 80.))),
        Draw::Shape(rect, 0., 0., [1., 0., 0., 0.5]),
        Draw::Clip(None),
    ];
    let mut layered = direct.clone();
    layered[2] = Draw::Shape(rect, 0., 0., [1., 0., 0., 1.]);
    // Resizing one target exercises cache replacement as well as both margin directions.
    let read_pixels = |size: Size| {
        let mut pixels = vec![0u8; (size.width * size.height * 4) as usize];
        unsafe {
            gl::ReadPixels(
                0,
                0,
                size.width as i32,
                size.height as i32,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                pixels.as_mut_ptr().cast(),
            );
        }
        pixels
    };
    for size in [
        Size::new(1280, 800),
        Size::new(1600, 720),
        Size::new(1280, 720),
    ] {
        window.set_size(size.width, size.height).unwrap();
        let (width, height) = window.drawable_size();
        let size = Size::new(width, height);
        renderer.set_layers(&[]);
        renderer.render(&direct, width, height, None);
        let expected = read_pixels(size);
        assert!(expected.chunks_exact(4).any(|pixel| pixel[0] > 80));
        renderer.set_layers(&[Layer {
            start: 1,
            end: layered.len(),
            alpha: 0.5,
        }]);
        renderer.render(&layered, width, height, None);
        assert!(
            expected == read_pixels(size),
            "layer pixels differ: {size:?}"
        );
        renderer.set_layers(&[]);
        renderer.render(&direct, width, height, None);
        assert!(
            expected == read_pixels(size),
            "ending layer jumps: {size:?}"
        );
    }
}
