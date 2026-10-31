use super::*;
use crate::{model::BACKGROUNDS, renderer::Draw, wallpapers::Wallpaper};

fn custom(index: u32) -> Wallpaper {
    Wallpaper {
        id: format!("custom-{index:032x}"),
        directory: PathBuf::from(format!("/preview/wallpapers/custom-{index:032x}")),
        has_original: true,
    }
}

#[test]
fn upload_sits_left_of_rotation_and_both_remain_fixed_in_every_language() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    for language in crate::i18n::Language::ALL {
        state.preferences.language = language;
        state.scroll = 0.;
        let before = controls(&state);
        let cycle = before
            .iter()
            .find(|c| c.target == Target::WallpaperCycle)
            .unwrap()
            .rect;
        let upload = before
            .iter()
            .find(|c| c.target == Target::UploadWallpaper)
            .unwrap()
            .rect;
        assert!(cycle.x >= 640.);
        let gradient = before
            .iter()
            .find(|c| c.target == Target::WallpaperGradient)
            .unwrap()
            .rect;
        assert!(upload.x >= 640., "{}: {}", language.code(), upload.x);
        assert_eq!(upload.x + upload.w + 20., gradient.x);
        assert_eq!(gradient.x + gradient.w + 20., cycle.x);
        assert_eq!(cycle.y, upload.y);
        state.scroll = 500.;
        let after = controls(&state);
        for target in [
            Target::WallpaperCycle,
            Target::WallpaperGradient,
            Target::UploadWallpaper,
        ] {
            let old = before.iter().find(|c| c.target == target).unwrap();
            let new = after.iter().find(|c| c.target == target).unwrap();
            assert_eq!(
                (old.rect.x, old.rect.y, old.rect.w),
                (new.rect.x, new.rect.y, new.rect.w)
            );
            assert!(!new.scrollable);
        }
    }
}

#[test]
fn cycling_button_toggles_on_off_without_opening_a_page_or_moving_controls() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::UploadWallpaper;
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::WallpaperGradient);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::WallpaperCycle);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::WallpaperGradient);
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::UploadWallpaper);
    state.direction(Direction::Right, &controls(&state));
    state.direction(Direction::Right, &controls(&state));
    assert!(!state.preferences.wallpaper_cycle);
    state.scroll = 300.;
    for language in crate::i18n::Language::ALL {
        state.preferences.language = language;
        let before = controls(&state);
        assert!(scene(&state).draw.iter().any(|draw| matches!(draw, Draw::TintedImage(path, ..) if path.ends_with("ui-icons/refresh-off.lvec"))));
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert!(state.preferences.wallpaper_cycle);
        assert!(state.preferences.wallpaper_changed > 0);
        assert!(state.subpage.is_none());
        assert!(state.overlay.is_none());
        assert_eq!(state.focus, Target::WallpaperCycle);
        assert_eq!(state.scroll, 300.);
        let enabled = scene(&state);
        assert!(enabled.draw.iter().any(|draw| matches!(draw, Draw::TintedImage(path, ..) if path.ends_with("ui-icons/refresh.lvec"))));
        assert!(!enabled.draw.iter().any(|draw| matches!(draw, Draw::TintedImage(path, ..) if path.ends_with("ui-icons/refresh-off.lvec"))));
        let label = language.t("wallpaperCycle");
        assert!(
            enabled
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(value, ..) if value == label))
        );
        let button = enabled
            .controls
            .iter()
            .find(|c| c.target == Target::WallpaperCycle)
            .unwrap()
            .rect;
        let title = enabled
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(value, rect, ..) if value == label => Some(rect),
                _ => None,
            })
            .unwrap();
        let (value, style) = enabled
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(label, rect, style, ..)
                    if label == language.t("stateOn") && rect.x > button.x =>
                {
                    Some((rect, style))
                }
                _ => None,
            })
            .unwrap();
        assert!(style.align_right);
        assert!((value.x - title.x - title.w - 24.).abs() < 0.001);
        assert!((button.x + button.w - value.x - value.w - 24.2).abs() < 0.001);
        for (old, new) in before.iter().zip(&enabled.controls) {
            assert_eq!(old.target, new.target);
            assert_eq!(
                (old.rect.x, old.rect.y, old.rect.w, old.rect.h),
                (new.rect.x, new.rect.y, new.rect.w, new.rect.h)
            );
        }
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert!(!state.preferences.wallpaper_cycle);
    }
}

#[test]
fn gradient_toggle_is_saved_stable_and_switches_bundled_and_uploaded_images() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::WallpaperGradient;
    state.scroll = 300.;
    let defaults: crate::model::Preferences = serde_json::from_str("{}").unwrap();
    assert!(defaults.wallpaper_gradient);
    for language in crate::i18n::Language::ALL {
        state.preferences.language = language;
        let before = controls(&state);
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert!(!state.preferences.wallpaper_gradient);
        assert_eq!(state.focus, Target::WallpaperGradient);
        assert_eq!(state.scroll, 300.);
        assert!(state.overlay.is_none() && state.subpage.is_none());
        let rendered = scene(&state);
        assert!(
            matches!(&rendered.draw[0], Draw::Image(path, ..) if path.ends_with("dunes-plain.jpg"))
        );
        for (old, new) in before.iter().zip(&rendered.controls) {
            assert_eq!(old.target, new.target);
            assert_eq!(
                (old.rect.x, old.rect.y, old.rect.w),
                (new.rect.x, new.rect.y, new.rect.w)
            );
        }
        let saved = serde_json::to_string(&state.preferences).unwrap();
        let restored: crate::model::Preferences = serde_json::from_str(&saved).unwrap();
        assert!(!restored.wallpaper_gradient);
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert!(
            matches!(&scene(&state).draw[0], Draw::Image(path, ..) if path.ends_with("dunes.jpg"))
        );
    }
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(1));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WallpaperGradient);
    let item = custom(1);
    state.preferences.background = item.id.clone();
    state.set_wallpapers(vec![item]);
    state.activate();
    assert!(
        matches!(&scene(&state).draw[0], Draw::Image(path, ..) if path.ends_with("original.jpg"))
    );
    state.wallpapers[0].has_original = false;
    assert!(
        matches!(&scene(&state).draw[0], Draw::Image(path, ..) if path.ends_with("wallpaper.jpg"))
    );
}

#[test]
fn cycling_uses_all_uploaded_ids_but_not_deleted_or_pending_deletion_entries() {
    let mut state = ready();
    state.preferences.reduced_motion = true;
    let first = custom(1);
    let second = custom(2);
    state.set_wallpapers(vec![first.clone(), second.clone()]);
    state.preferences.background = "northern-lights".into();
    state.deleting_wallpaper = Some(first.id.clone());
    assert!(state.cycle_wallpaper(100));
    assert_eq!(state.preferences.background, second.id);
    assert!(!state.can_delete_background(&second.id));
    assert!(state.previous_background.is_none());
    assert!(state.cycle_wallpaper(200));
    assert_eq!(state.preferences.background, "slate");
    state.deleting_wallpaper = None;
    state.preferences.background = "northern-lights".into();
    assert!(state.cycle_wallpaper(300));
    assert_eq!(state.preferences.background, first.id);
    state.preferences.background = "dunes".into();
    state.preferences.deleted_backgrounds = vec!["san-francisco".into()];
    assert!(state.cycle_wallpaper(400));
    assert_eq!(state.preferences.background, "summit");
}

#[test]
fn automatic_changes_preserve_manual_splash_across_saves_and_restart_and_manual_selection_resets_time()
 {
    let mut state = ready();
    state.preferences.background = "forest".into();
    state.preferences.wallpaper_cycle = true;
    state.preferences.wallpaper_changed = 100;
    assert!(!crate::wallpaper_cycle::due(true, 100, 100));
    assert!(crate::wallpaper_cycle::startup_due(
        true,
        true,
        true,
        std::time::Duration::ZERO
    ));
    assert!(state.cycle_wallpaper(100));
    assert_eq!(state.background_fade, crate::state::CYCLE_FADE);
    assert_eq!(state.preferences.splash_background(), "forest");
    state.preferences.show_seconds = !state.preferences.show_seconds;
    let mut preferences: Preferences =
        serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
    preferences.sanitize();
    assert_eq!(preferences.splash_background(), "forest");
    let mut restored = State::new(preferences);
    assert!(restored.cycle_wallpaper(200));
    assert_eq!(restored.preferences.splash_background(), "forest");
    restored.open(2);
    restored.section = 1;
    restored.focus = Target::Background(0);
    restored.activate();
    assert_eq!(restored.activate(), vec![Effect::Save]);
    assert_eq!(restored.preferences.background, "slate");
    assert_eq!(restored.preferences.splash_background(), "slate");
    assert_eq!(restored.background_fade, crate::state::BACKGROUND_FADE);
    assert!(restored.preferences.wallpaper_changed > 200);
}

#[test]
fn former_exclusions_are_ignored_and_only_the_on_off_state_is_saved() {
    let preferences: Preferences = serde_json::from_value(serde_json::json!({
        "wallpaper_cycle": "hourly", "wallpaper_skip": ["san-francisco"], "background": "dunes"
    }))
    .unwrap();
    let mut state = State::new(preferences);
    assert!(state.preferences.wallpaper_cycle);
    assert!(state.cycle_wallpaper(100));
    assert_eq!(state.preferences.background, "san-francisco");
    let saved = serde_json::to_value(state.preferences).unwrap();
    assert_eq!(saved["wallpaper_cycle"], true);
    assert!(saved.get("wallpaper_skip").is_none());
}

#[test]
fn upload_is_modal_and_close_restores_its_exact_parent() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::Background(0);
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::UploadWallpaper);
    assert!(state.activate().is_empty());
    assert!(matches!(state.overlay, Some(Overlay::WallpaperUpload)));
    assert_eq!(state.focus, Target::CloseDialog);
    let modal = scene(&state);
    assert!(modal.modal.is_some());
    assert_eq!(modal.controls.len(), 1);
    assert_eq!(modal.controls[0].target, Target::CloseDialog);
    state.direction(Direction::Right, &modal.controls);
    assert_eq!(state.focus, Target::CloseDialog);
    state.activate();
    assert!(state.overlay.is_none());
    assert_eq!(state.focus, Target::UploadWallpaper);
    assert_eq!(state.section, 1);
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(0));
}

#[test]
fn upload_stays_active_without_desktop_focus_but_requires_tv_focus() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::UploadWallpaper;
    state.activate();

    for webos in [false, true] {
        assert!(crate::wallpaper_upload_active(&state, webos, true, true));
        assert_eq!(
            crate::wallpaper_upload_active(&state, webos, false, true),
            !webos
        );
        for focused in [false, true] {
            assert!(!crate::wallpaper_upload_active(
                &state, webos, focused, false
            ));
        }
    }

    state.back();
    for webos in [false, true] {
        for focused in [false, true] {
            assert!(!crate::wallpaper_upload_active(
                &state, webos, focused, true
            ));
        }
    }
}

#[test]
fn upload_instructions_are_ordered_and_only_the_displayed_url_loses_its_slash() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::UploadWallpaper;
    state.activate();
    let url = "http://192.168.100.100:54321/AbCd1234/";
    state.wallpaper_upload = Some(Ok(crate::wallpapers::Session {
        url: url.into(),
        qr_size: 21,
        qr: vec![false; 21 * 21],
    }));
    for language in crate::i18n::Language::ALL {
        state.preferences.language = language;
        let modal = scene(&state);
        let labels: Vec<_> = modal
            .draw
            .iter()
            .filter_map(|draw| match draw {
                Draw::Text(label, rect, ..) => Some((label.as_str(), rect)),
                _ => None,
            })
            .collect();
        let mut previous_bottom = 0.;
        for label in [
            language.t("wallpaperUploadHint"),
            language.t("wallpaperUploadBrowser"),
            url.trim_end_matches('/'),
            language.t("wallpaperUploadKeepOpen"),
            language.t("wallpaperUploadPrivate"),
            language.t("close"),
        ] {
            let (_, rect) = labels.iter().find(|(text, _)| *text == label).unwrap();
            assert!(rect.y >= previous_bottom);
            previous_bottom = rect.y + rect.h;
        }
        assert!(!labels.iter().any(|(label, _)| *label == url));
        assert_eq!(modal.controls.len(), 1);
        assert_eq!(modal.controls[0].target, Target::CloseDialog);
        let panel = modal.modal.unwrap();
        let close = modal.controls[0].rect;
        assert!(panel.y + panel.h - close.y - close.h >= 40.);
    }
    let session = state.wallpaper_upload.as_ref().unwrap().as_ref().unwrap();
    assert_eq!(session.url, url);
}

#[test]
fn upload_button_is_compact_and_rotation_is_right_aligned_in_every_language() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    for language in crate::i18n::Language::ALL {
        state.preferences.language = language;
        let rendered = scene(&state);
        let gallery = &rendered.controls;
        let upload = gallery
            .iter()
            .find(|c| c.target == Target::UploadWallpaper)
            .unwrap()
            .rect;
        let right_card = gallery
            .iter()
            .find(|c| c.target == Target::Background(2))
            .unwrap()
            .rect;
        let cycle = gallery
            .iter()
            .find(|c| c.target == Target::WallpaperCycle)
            .unwrap()
            .rect;
        assert_eq!(cycle.x + cycle.w, right_card.x + right_card.w);
        assert!((88. ..=520.).contains(&upload.w));
        assert!(upload.y + upload.h < right_card.y);
        let title = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(label, rect, ..) if label == language.t("uploadWallpaper") => Some(rect),
                _ => None,
            })
            .unwrap();
        assert!((upload.x + upload.w - title.x - title.w - 24.2).abs() < 0.01);
    }
}

#[test]
fn gallery_scrolls_behind_fixed_controls_without_clipping_or_fades() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.set_wallpapers((1..=8).map(custom).collect());
    let before = controls(&state);
    state.wheel_scroll(&before, Some((1000., 600.)), 0, -3);
    assert!(state.scroll > 0.);
    let scrolled = scene(&state);
    for target in [
        Target::UploadWallpaper,
        Target::Sidebar(0),
        Target::Sidebar(1),
        Target::Sidebar(crate::state::SYSTEM_SECTION),
    ] {
        let old = before.iter().find(|c| c.target == target).unwrap();
        let new = scrolled
            .controls
            .iter()
            .find(|c| c.target == target)
            .unwrap();
        assert_eq!(
            (old.rect.x, old.rect.y, old.rect.w, old.rect.h),
            (new.rect.x, new.rect.y, new.rect.w, new.rect.h)
        );
        assert!(!new.scrollable);
    }
    let old = before
        .iter()
        .find(|c| c.target == Target::Background(3))
        .unwrap();
    let new = scrolled
        .controls
        .iter()
        .find(|c| c.target == Target::Background(3))
        .unwrap();
    assert_eq!(old.rect.y - new.rect.y, state.scroll);
    assert!(!scrolled.draw.iter().any(|draw| matches!(
        draw,
        Draw::Clip(Some(_)) | Draw::EdgeFade(Some(_)) | Draw::EdgeFadeX(Some(_))
    )));
    let last_card = scrolled
        .draw
        .iter()
        .rposition(|draw| {
            matches!(draw,
                Draw::Image(_, rect, ..) if rect.w == 360. && rect.h == 202.5
            )
        })
        .unwrap();
    for key in ["uploadWallpaper", "wallpaperCycle", "interface"] {
        let label = state.preferences.language.t(key);
        let fixed = scrolled
            .draw
            .iter()
            .position(|draw| {
                matches!(draw,
                    Draw::Text(text, ..) if text == label
                )
            })
            .unwrap();
        assert!(fixed > last_card, "{key} must be drawn above the gallery");
    }
    let scroll = state.scroll;
    state.wheel_scroll(&scrolled.controls, Some((300., 300.)), 0, -3);
    assert_eq!(state.scroll, scroll);
}

#[test]
fn scrolled_backgrounds_cannot_capture_upload_or_sidebar_pointer_hits() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.scroll = 150.;
    let gallery = controls(&state);
    let upload = gallery
        .iter()
        .find(|c| c.target == Target::UploadWallpaper)
        .unwrap()
        .rect;
    let (x, y) = upload.center();
    assert_eq!(
        state.hit_test(&gallery, x, y),
        Some(Target::UploadWallpaper)
    );
    assert_eq!(state.hit_test(&gallery, 1000., 200.), None);
    assert_eq!(
        state.hit_test(&gallery, 300., 280.),
        Some(Target::Sidebar(1))
    );
    assert_eq!(
        state.hit_test(&gallery, 900., 260.),
        Some(Target::Background(0))
    );
}

#[test]
fn remote_background_focus_stays_below_the_fixed_upload_action() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.set_wallpapers((1..=9).map(custom).collect());
    state.focus = Target::Background(2);
    for _ in 0..5 {
        state.direction(Direction::Down, &controls(&state));
    }
    assert!(state.scroll > 0.);
    for _ in 0..5 {
        state.direction(Direction::Up, &controls(&state));
        if matches!(state.focus, Target::Background(_)) {
            let gallery = controls(&state);
            let focused = gallery.iter().find(|c| c.target == state.focus).unwrap();
            let viewport = state.backgrounds_viewport();
            assert!(focused.rect.y >= viewport.y);
            assert!(focused.rect.y + focused.rect.h <= viewport.y + viewport.h);
        }
    }
    assert_eq!(state.focus, Target::Background(2));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::WallpaperCycle);
}

#[test]
fn custom_gallery_navigation_reveals_whole_cards_in_incomplete_rows() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.set_wallpapers(vec![custom(1), custom(2)]);
    state.focus = Target::Background(8);
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Background(9));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Background(10));
    state.direction(Direction::Right, &controls(&state));
    assert_eq!(state.focus, Target::Background(10));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(10));
    state.direction(Direction::Up, &controls(&state));
    assert_eq!(state.focus, Target::Background(7));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Background(10));
    let focused = controls(&state)
        .into_iter()
        .find(|c| c.target == state.focus)
        .unwrap();
    let viewport = state.content_viewport();
    assert!(focused.rect.y >= viewport.y);
    assert!(focused.rect.y + focused.rect.h <= viewport.y + viewport.h);
}

#[test]
fn upload_discovery_does_not_apply_and_custom_selection_uses_immutable_paths() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    let item = custom(1);
    state.set_wallpapers(vec![item.clone()]);
    assert_eq!(state.preferences.background, "dunes");
    let gallery = scene(&state);
    assert!(
        gallery
            .draw
            .iter()
            .any(|draw| matches!(draw, Draw::Image(path, ..) if path == &item.thumbnail()))
    );
    state.focus = Target::Background(BACKGROUNDS.len());
    assert!(state.activate().is_empty());
    assert_eq!(state.focus, Target::UseBackground);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.background, item.id);
    assert!(
        scene(&state)
            .draw
            .iter()
            .any(|draw| matches!(draw, Draw::TintedImage(path, ..) if path == &item.image()))
    );
    state.previous_background = None;
    assert!(matches!(&scene(&state).draw[0], Draw::Image(path, ..) if path == &item.image()));
    let mut restored = State::new(
        serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap(),
    );
    restored.preferences.sanitize();
    assert_eq!(restored.preferences.background, item.id);
    // Missing uploads fall back without overwriting the saved selection.
    assert!(
        matches!(&scene(&restored).draw[0], Draw::Image(path, ..) if path.ends_with("wallpapers/dunes.jpg"))
    );
    restored.set_wallpapers(vec![item.clone()]);
    assert!(matches!(&scene(&restored).draw[0], Draw::Image(path, ..) if path == &item.image()));
}

#[test]
fn catalog_updates_keep_the_focused_image_and_pointer_geometry_stationary() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    let item = custom(1);
    state.set_wallpapers(vec![item.clone()]);
    state.focus = Target::Background(9);
    state.set_wallpapers(vec![custom(2), item.clone()]);
    assert_eq!(state.focus, Target::Background(10));
    assert_eq!(state.background_id(10), Some(item.id.as_str()));
    let scroll = state.scroll;
    let before = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::Background(10))
        .unwrap();
    state.pointer_focus(Target::Background(10));
    assert_eq!(state.scroll, scroll);
    let after = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::Background(10))
        .unwrap();
    assert_eq!((before.rect.x, before.rect.y), (after.rect.x, after.rect.y));
}

#[test]
fn background_popup_is_modal_lifts_the_same_thumbnail_and_restores_its_parent() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.focus = Target::Background(0);
    let before = controls(&state)
        .into_iter()
        .find(|c| c.target == state.focus)
        .unwrap()
        .rect;
    assert!(state.activate().is_empty());
    assert_eq!(state.focus, Target::UseBackground);
    assert!(matches!(
        state.overlay,
        Some(Overlay::BackgroundOptions { .. })
    ));
    let popup = scene(&state);
    assert!(popup.modal.is_some());
    assert_eq!(
        popup
            .controls
            .iter()
            .map(|c| c.target.clone())
            .collect::<Vec<_>>(),
        vec![Target::UseBackground, Target::DeleteBackground]
    );
    assert!(popup.draw.iter().any(|draw| matches!(draw, Draw::Image(_, rect, ..) if rect.x == before.x && rect.y == before.y && rect.w == before.w && rect.h == 202.5)));
    state.direction(Direction::Down, &popup.controls);
    assert_eq!(state.focus, Target::DeleteBackground);
    state.activate();
    assert_eq!(state.focus, Target::CancelDelete);
    assert!(matches!(
        state.overlay,
        Some(Overlay::BackgroundDelete { .. })
    ));
    state.activate();
    assert_eq!(state.focus, Target::DeleteBackground);
    state.back();
    assert_eq!(state.focus, Target::Background(0));
    assert!(state.overlay.is_none());
}

#[test]
fn selected_background_has_no_delete_action_and_rejects_forced_confirmation() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    let selected = state.preferences.background.clone();
    let index = (0..state.background_count())
        .find(|&i| state.background_id(i) == Some(&selected))
        .unwrap();
    state.focus = Target::Background(index);
    state.options();
    assert_eq!(scene(&state).controls.len(), 1);
    assert!(!state.can_delete_background(&selected));
    state.focus = Target::DeleteBackground;
    state.activate();
    assert!(matches!(
        state.overlay,
        Some(Overlay::BackgroundOptions { .. })
    ));
    state.overlay = Some(Overlay::BackgroundDelete {
        id: selected.clone(),
        restore: Target::Background(index),
    });
    state.focus = Target::ConfirmDelete;
    assert!(state.activate().is_empty());
    assert_eq!(state.preferences.background, selected);
    assert!(state.preferences.deleted_backgrounds.is_empty());
}

#[test]
fn every_unselected_bundled_background_can_be_removed_and_stays_removed_after_reload() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    for (id, _) in BACKGROUNDS {
        if *id == state.preferences.background {
            continue;
        }
        let index = (0..state.background_count())
            .find(|&i| state.background_id(i) == Some(id))
            .unwrap();
        state.focus = Target::Background(index);
        state.activate();
        state.focus = Target::DeleteBackground;
        state.activate();
        state.focus = Target::ConfirmDelete;
        assert_eq!(state.activate(), vec![Effect::Save]);
        assert!(state.overlay.is_none());
        assert!(!(0..state.background_count()).any(|i| state.background_id(i) == Some(id)));
        assert!(controls(&state).iter().any(|c| c.target == state.focus));
    }
    assert_eq!(state.background_count(), 1);
    let mut preferences: crate::model::Preferences =
        serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
    preferences.sanitize();
    let restored = State::new(preferences);
    assert_eq!(restored.background_count(), 1);
    assert_eq!(restored.background_id(0), Some("dunes"));
}

#[test]
fn uploaded_deletion_is_single_flight_handles_failures_and_keeps_unrelated_focus() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    let lake = custom(1);
    let mountain = custom(2);
    state.set_wallpapers(vec![lake.clone(), mountain.clone()]);
    state.focus = Target::Background(BACKGROUNDS.len());
    state.activate();
    state.focus = Target::DeleteBackground;
    state.activate();
    state.focus = Target::ConfirmDelete;
    assert_eq!(
        state.activate(),
        vec![Effect::DeleteWallpaper(lake.id.clone())]
    );
    state.focus = Target::ConfirmDelete;
    assert!(state.activate().is_empty());
    state.wallpaper_deleted(mountain.id.clone(), Ok(()));
    assert_eq!(state.wallpapers.len(), 2);
    state.wallpaper_deleted(lake.id.clone(), Err("read-only".into()));
    assert_eq!(state.wallpapers.len(), 2);
    assert!(state.deleting_wallpaper.is_none());
    state.focus = Target::ConfirmDelete;
    assert_eq!(
        state.activate(),
        vec![Effect::DeleteWallpaper(lake.id.clone())]
    );
    state.back();
    state.focus = Target::UseBackground;
    assert!(state.activate().is_empty()); // Cannot apply an image being deleted.
    assert_eq!(state.preferences.background, "dunes");
    state.back();
    state.focus = Target::Background(BACKGROUNDS.len() + 1);
    state.wallpaper_deleted(lake.id, Ok(()));
    assert_eq!(state.focus, Target::Background(BACKGROUNDS.len()));
    assert_eq!(
        state.background_id(BACKGROUNDS.len()),
        Some(mountain.id.as_str())
    );
    assert!(state.overlay.is_none());
}

#[test]
fn filtered_bundled_entries_do_not_misidentify_custom_thumbnails_or_popup_parents() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.preferences.deleted_backgrounds = BACKGROUNDS
        .iter()
        .filter(|(id, _)| *id != "dunes")
        .map(|(id, _)| (*id).into())
        .collect();
    let lake = custom(1);
    state.set_wallpapers(vec![lake.clone()]);
    state.focus = Target::Background(1);
    assert!(
        scene(&state)
            .draw
            .iter()
            .any(|draw| matches!(draw, Draw::Image(path, ..) if path == &lake.thumbnail()))
    );
    state.options();
    state.set_wallpapers(vec![custom(2), lake.clone()]);
    state.back();
    assert_eq!(state.focus, Target::Background(2));
    state.options();
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert_eq!(state.preferences.background, lake.id);
}

#[test]
fn removed_popup_entry_closes_safely_and_preference_sanitizing_protects_the_selection() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.set_wallpapers(vec![custom(1)]);
    state.focus = Target::Background(BACKGROUNDS.len());
    state.options();
    state.set_wallpapers(vec![]);
    assert!(state.overlay.is_none());
    assert!(controls(&state).iter().any(|c| c.target == state.focus));
    state.preferences.deleted_backgrounds = vec![
        "dunes".into(),
        "../bad".into(),
        "forest".into(),
        "forest".into(),
    ];
    state.preferences.sanitize();
    assert!(
        !state
            .preferences
            .deleted_backgrounds
            .iter()
            .any(|id| id == "dunes" || id == "../bad")
    );
    assert!(state.preferences.deleted_backgrounds.len() <= 1);
}
