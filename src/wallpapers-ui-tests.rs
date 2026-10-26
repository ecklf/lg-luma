use super::*;
use crate::{model::BACKGROUNDS, renderer::Draw, wallpapers::Wallpaper};

fn custom(index: u32, name: &str) -> Wallpaper {
    Wallpaper {
        id: format!("custom-{index:032x}"),
        name: name.into(),
        directory: PathBuf::from(format!("/preview/wallpapers/custom-{index:032x}")),
    }
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
fn custom_gallery_navigation_reveals_whole_cards_in_incomplete_rows() {
    let mut state = ready();
    state.open(2);
    state.section = 1;
    state.set_wallpapers(vec![custom(1, "Lake"), custom(2, "Mountains")]);
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
    let item = custom(1, "Summer lake");
    state.set_wallpapers(vec![item.clone()]);
    assert_eq!(state.preferences.background, "dunes");
    let gallery = scene(&state);
    assert!(
        gallery
            .draw
            .iter()
            .any(|draw| matches!(draw, Draw::Text(title, ..) if title == "Summer lake"))
    );
    state.focus = Target::Background(BACKGROUNDS.len());
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
    let item = custom(1, "Lake");
    state.set_wallpapers(vec![item.clone()]);
    state.focus = Target::Background(9);
    state.set_wallpapers(vec![custom(2, "Alps"), item.clone()]);
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
