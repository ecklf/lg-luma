use super::*;

#[test]
fn multisample_resolve_requires_a_supported_context_version() {
    for version in [
        "OpenGL ES 2.0 vendor",
        "2.1 Mesa",
        "OpenGL ES-CM 1.1",
        "unknown",
    ] {
        assert!(!layer_resolve_version(version), "{version}");
    }
    for version in [
        "OpenGL ES 3.0 vendor",
        "OpenGL ES 3.2 v1.r11p0",
        "3.3 Mesa",
        "4.6 vendor",
    ] {
        assert!(layer_resolve_version(version), "{version}");
    }
}

#[test]
fn pass_work_does_not_replace_window_metadata_or_double_count_cache_snapshots() {
    let mut window = RenderStats::new(Size::HD);
    window.multisample_samples = 4;
    window.redrawn_fraction = 0.25;
    window.cached_glyphs = 12;
    let mut pass = RenderStats::new(Size::new(1280, 720));
    pass.glyph_tessellations = 3;
    pass.glyph_ms = 2.;
    pass.vector_load_ms = 1.;
    pass.image_load_ms = 4.;
    pass.glass_precomputed_loads = 1;
    pass.draw_calls = 5;
    pass.cached_glyphs = 12;
    window.include_work(&pass);
    assert_eq!(window.target, Size::HD);
    assert_eq!(window.multisample_samples, 4);
    assert_eq!(window.redrawn_fraction, 0.25);
    assert_eq!(window.cached_glyphs, 12);
    assert_eq!(window.glyph_tessellations, 3);
    assert_eq!(window.glyph_ms, 2.);
    assert_eq!(window.vector_load_ms, 1.);
    assert_eq!(window.image_load_ms, 4.);
    assert_eq!(window.glass_precomputed_loads, 1);
    assert_eq!(window.draw_calls, 5);
}

fn with_renderer(gles: bool, samples: u8, check: impl FnOnce(&mut Renderer, GLuint)) {
    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    video.gl_attr().set_context_profile(if gles {
        sdl2::video::GLProfile::GLES
    } else {
        sdl2::video::GLProfile::Compatibility
    });
    video.gl_attr().set_context_version(3, 0);
    video
        .gl_attr()
        .set_multisample_buffers(u8::from(samples > 0));
    video.gl_attr().set_multisample_samples(samples);
    let window = video
        .window("Layer rendering regression", 1280, 720)
        .opengl()
        .hidden()
        .build()
        .unwrap();
    let _context = window.gl_create_context().unwrap();
    gl::load_with(|name| video.gl_get_proc_address(name) as *const _);
    assert_eq!(target_samples(), u32::from(samples));
    // Use identical RGBA8 FBO formats/sample locations for reference and layer.
    // Window-system MSAA buffers can have different sample positions/readback rules.
    let (mut target, mut color) = (0, 0);
    if samples > 0 {
        unsafe {
            gl::GenFramebuffers(1, &mut target);
            gl::GenRenderbuffers(1, &mut color);
            gl::BindRenderbuffer(gl::RENDERBUFFER, color);
            gl::RenderbufferStorageMultisample(
                gl::RENDERBUFFER,
                i32::from(samples),
                gl::RGBA8,
                1280,
                720,
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, target);
            gl::FramebufferRenderbuffer(
                gl::FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::RENDERBUFFER,
                color,
            );
            assert_eq!(
                gl::CheckFramebufferStatus(gl::FRAMEBUFFER),
                gl::FRAMEBUFFER_COMPLETE
            );
        }
    }
    let mut renderer = Renderer::new(Path::new(".build/assets/font-400.ttf"), gles, 0.).unwrap();
    check(&mut renderer, target);
    drop(renderer);
    unsafe {
        gl::BindFramebuffer(gl::FRAMEBUFFER, 0);
        gl::DeleteFramebuffers(1, &target);
        gl::DeleteRenderbuffers(1, &color);
    }
    assert_eq!(unsafe { gl::GetError() }, gl::NO_ERROR);
}

fn pixels() -> Vec<u8> {
    let mut pixels = vec![0; 1280 * 720 * 4];
    let samples = target_samples();
    let mut previous = 0;
    let mut resolve = 0;
    let resolved = (samples > 1).then(|| texture(1280, 720, &[]));
    unsafe {
        gl::GetIntegerv(gl::FRAMEBUFFER_BINDING, &mut previous);
        if let Some(image) = &resolved {
            gl::GenFramebuffers(1, &mut resolve);
            gl::BindFramebuffer(gl::DRAW_FRAMEBUFFER, resolve);
            gl::FramebufferTexture2D(
                gl::DRAW_FRAMEBUFFER,
                gl::COLOR_ATTACHMENT0,
                gl::TEXTURE_2D,
                image.id,
                0,
            );
            gl::BindFramebuffer(gl::READ_FRAMEBUFFER, previous as GLuint);
            gl::BlitFramebuffer(
                0,
                0,
                1280,
                720,
                0,
                0,
                1280,
                720,
                gl::COLOR_BUFFER_BIT,
                gl::NEAREST,
            );
            gl::BindFramebuffer(gl::FRAMEBUFFER, resolve);
        }
        gl::ReadPixels(
            0,
            0,
            1280,
            720,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            pixels.as_mut_ptr().cast(),
        );
        gl::BindFramebuffer(gl::FRAMEBUFFER, previous as GLuint);
        gl::DeleteFramebuffers(1, &resolve);
    }
    pixels
}

fn assert_pixels(expected: &[u8]) {
    let largest = expected
        .iter()
        .zip(pixels())
        .map(|(&a, b)| a.abs_diff(b))
        .max()
        .unwrap();
    assert!(
        largest <= 1,
        "layer differs from direct rendering by {largest}/255"
    );
}

fn text_scene() -> Vec<Draw> {
    vec![
        Draw::Shape(Rect::new(0., 0., 1920., 1080.), 0., 0., [0., 0., 0., 1.]),
        Draw::Text(
            "Settings • Refresh apps & inputs • Thin glyphs WWW".into(),
            Rect::new(300., 300., 1200., 80.),
            TextStyle::new(22.),
            [1.; 4],
            false,
        ),
    ]
}

#[test]
#[ignore = "requires an OpenGL display and generated font assets"]
fn layer_text_preserves_msaa_and_analytic_pixels() {
    for gles in [false, true] {
        for samples in [0, 4] {
            with_renderer(gles, samples, |renderer, target| {
                let scene = text_scene();
                for alpha in [1., 0.5] {
                    renderer.set_layers(&[]);
                    renderer.render(&scene, 1280, 720, None);
                    let full_opacity = pixels();
                    // A layer fades the resolved surface, not overlapping glyph triangles
                    // separately. Its reference is the selected AA's full-opacity pixels.
                    let mut expected = full_opacity.clone();
                    for pixel in expected.chunks_exact_mut(4) {
                        for channel in &mut pixel[..3] {
                            *channel = (f32::from(*channel) * alpha).round() as u8;
                        }
                    }
                    assert!(expected.chunks_exact(4).any(|pixel| pixel[0] > 80));
                    renderer.set_layers(&[Layer {
                        start: 1,
                        end: 2,
                        alpha,
                    }]);
                    unsafe { gl::BindFramebuffer(gl::READ_FRAMEBUFFER, 0) };
                    renderer.render(&scene, 1280, 720, None);
                    let mut restored_read = -1;
                    unsafe { gl::GetIntegerv(gl::READ_FRAMEBUFFER_BINDING, &mut restored_read) };
                    assert_eq!(
                        restored_read, 0,
                        "layer passes must restore the caller's read target"
                    );
                    assert_pixels(&expected);
                    assert_eq!(renderer.layer_caches[0].samples, u32::from(samples));
                    assert_eq!(renderer.stats().multisample_samples, u32::from(samples));
                    // Exercise reuse, and the endpoint where a Settings subpage draws directly.
                    renderer.render(&scene, 1280, 720, None);
                    assert_pixels(&expected);
                    renderer.set_layers(&[]);
                    renderer.render(&scene, 1280, 720, None);
                    assert_pixels(&full_opacity);
                }
                // Captures can temporarily use a single-sample target in an MSAA window.
                let capture_texture = texture(1280, 720, &[]);
                let mut capture = 0;
                unsafe {
                    gl::GenFramebuffers(1, &mut capture);
                    gl::BindFramebuffer(gl::FRAMEBUFFER, capture);
                    gl::FramebufferTexture2D(
                        gl::FRAMEBUFFER,
                        gl::COLOR_ATTACHMENT0,
                        gl::TEXTURE_2D,
                        capture_texture.id,
                        0,
                    );
                    assert_eq!(
                        gl::CheckFramebufferStatus(gl::FRAMEBUFFER),
                        gl::FRAMEBUFFER_COMPLETE
                    );
                }
                renderer.set_layers(&[Layer {
                    start: 1,
                    end: 2,
                    alpha: 1.,
                }]);
                renderer.render(&scene, 1280, 720, None);
                assert_eq!(renderer.layer_caches[0].samples, 0);
                unsafe { gl::BindFramebuffer(gl::FRAMEBUFFER, target) };
                renderer.render(&scene, 1280, 720, None);
                assert_eq!(renderer.layer_caches[0].samples, u32::from(samples));
                unsafe { gl::DeleteFramebuffers(1, &capture) };
                if samples > 1 {
                    // The GLES 2/no-resolve policy must not silently select analytic text.
                    renderer.set_layers(&[]);
                    renderer.render(&scene, 1280, 720, None);
                    let expected = pixels();
                    renderer.layer_multisample = false;
                    renderer.set_layers(&[Layer {
                        start: 1,
                        end: 2,
                        alpha: 1.,
                    }]);
                    renderer.render(&scene, 1280, 720, None);
                    assert_pixels(&expected);
                    assert_eq!(renderer.stats().multisample_samples, u32::from(samples));
                }
            });
        }
    }
}

#[test]
#[ignore = "requires an OpenGL display and generated font assets"]
fn layer_diagnostics_include_stale_pass_work_but_not_reused_work() {
    for samples in [0, 4] {
        with_renderer(true, samples, |renderer, _| {
            let mut scene = text_scene();
            scene.insert(
                1,
                Draw::Glass(Rect::new(280., 280., 1280., 120.), 20., [0.1, 0.1, 0.1, 1.]),
            );
            renderer.set_layers(&[Layer {
                start: 1,
                end: scene.len(),
                alpha: 0.5,
            }]);
            renderer.render(&scene, 1280, 720, None);
            let first = renderer.stats().clone();
            assert_eq!(first.glass_surfaces, 1);
            assert!(first.glyph_tessellations > 0);
            assert!(first.glyph_ms > 0.);
            assert!(first.cpu_submission_ms >= first.glyph_ms);
            assert_eq!(first.target, Size::new(1280, 720));
            assert_eq!(first.multisample_samples, u32::from(samples));
            renderer.render(&scene, 1280, 720, Some([0, 0, 640, 360]));
            let reused = renderer.stats();
            assert_eq!(reused.glass_surfaces, 1);
            assert_eq!(reused.glyph_tessellations, 0);
            assert_eq!(reused.glyph_ms, 0.);
            assert_eq!(reused.redrawn_fraction, 0.25);
            assert!(reused.draw_calls < first.draw_calls);
            if let Draw::Text(text, ..) = &mut scene[2] {
                *text = "New glyphs: QZ".into();
            }
            renderer.render(&scene, 1280, 720, None);
            assert!(renderer.stats().glyph_tessellations > 0);
            assert_eq!(renderer.stats().glass_surfaces, 1);
            // Both cached pages share one scratch MSAA target; rendering the second
            // must not overwrite the first page's resolved texture.
            renderer.set_layers(&[
                Layer {
                    start: 1,
                    end: 2,
                    alpha: 0.5,
                },
                Layer {
                    start: 2,
                    end: 3,
                    alpha: 0.5,
                },
            ]);
            renderer.render(&scene, 1280, 720, None);
            renderer.render(&scene, 1280, 720, None);
            assert_eq!(renderer.layer_caches.len(), 2);
            let stable = pixels();
            renderer.render(&scene, 1280, 720, None);
            assert_pixels(&stable);
            assert_eq!(renderer.stats().glass_surfaces, 1);
        });
    }
}

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
