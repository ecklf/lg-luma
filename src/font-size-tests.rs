use super::physical_font_size;

#[test]
fn fractional_1080p_sizes_use_whole_physical_pixels() {
    assert_eq!(physical_font_size(18.4, 1., true), 18.);
    assert_eq!(physical_font_size(90.4, 1., true), 90.);
    assert_eq!(physical_font_size(24., 1., true), 24.);
}

#[test]
fn quantization_uses_drawable_density_not_logical_pixels() {
    for scale in [0.5, 1., 1.25, 1.5, 2.] {
        for size in [0.2, 18.4, 24., 31.9, 90.4] {
            let logical = physical_font_size(size, scale, true);
            let physical = logical * scale;
            assert!((physical - physical.round()).abs() < 0.0001);
            assert!(physical >= 1.);
            if size * scale >= 1. {
                assert!(logical <= size + 0.0001, "Fitted text must not grow");
                assert!(size - logical < 1. / scale + 0.0001);
            }
        }
    }
    assert_eq!(physical_font_size(18.6, 2., true), 18.5);
}

#[test]
fn comparison_override_retains_original_sizes() {
    for scale in [0.5, 1., 1.25, 2.] {
        assert_eq!(physical_font_size(18.4, scale, false), 18.4);
    }
}

#[test]
fn effective_size_distinguishes_density_dependent_run_cache_entries() {
    let size_key = |scale| physical_font_size(18.6, scale, true).to_bits();
    assert_ne!(size_key(1.), size_key(2.));
    // Equal effective logical sizes may safely reuse geometry across densities.
    assert_eq!(size_key(1.), size_key(0.5));
}
