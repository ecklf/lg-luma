use super::*;
use crate::{
    markets,
    model::{HomeWidget, WidgetPosition},
    renderer::Draw,
    state::{Subpage, WIDGETS_SECTION},
};

#[test]
fn repeated_widgets_have_independent_configuration_placement_and_removal() {
    let mut state = ready();
    state.open(2);
    state.section = WIDGETS_SECTION;
    for kind in [HomeWidget::Weather, HomeWidget::Crypto, HomeWidget::Stocks] {
        for _ in 0..2 {
            let widget = state.preferences.available_widget(kind).unwrap();
            state.focus = Target::WidgetAdd;
            state.activate();
            assert!(
                controls(&state)
                    .iter()
                    .any(|control| control.target == Target::AddWidget(widget))
            );
            state.pointer_focus(Target::AddWidget(widget));
            assert!(state.activate().is_empty());
            assert!(!state.preferences.has_widget(widget));
            assert_eq!(state.activate(), [Effect::Save]);
            assert!(state.preferences.has_widget(widget));
            state.focus = Target::ConfigureWidget(widget);
            state.activate();
            if kind == HomeWidget::Weather {
                let name = format!("City {}", state.preferences.widget_position(widget).cell());
                state.weather.results = vec![crate::weather::Location {
                    name: name.clone(),
                    region: String::new(),
                    country: String::new(),
                    latitude: 48.,
                    longitude: 16.,
                }];
                state.focus = Target::WeatherResult(0);
                assert_eq!(state.activate(), [Effect::Save]);
                assert_eq!(
                    state
                        .preferences
                        .weather_settings(widget)
                        .location
                        .unwrap()
                        .name,
                    name
                );
                state.focus = Target::WeatherTemperature;
                state.activate();
                state.focus = Target::Choice(1);
                state.activate();
                assert_eq!(
                    state.preferences.weather_settings(widget).unit,
                    crate::weather::Unit::Fahrenheit
                );
            } else {
                state.focus = Target::MarketSymbol(widget);
                state.activate();
                state.text(if kind == HomeWidget::Crypto {
                    "ETH"
                } else {
                    "MSFT"
                });
                assert_eq!(state.back(), [Effect::Save]);
                if kind == HomeWidget::Crypto {
                    state.direction(Direction::Down, &controls(&state));
                    assert_eq!(state.focus, Target::MarketCurrency);
                    state.activate();
                    assert_eq!(
                        markets::settings(&state.preferences, widget).currency,
                        "EUR"
                    );
                }
            }
            state.back();
            assert_eq!(
                state.focus,
                Target::WidgetCell(state.preferences.widget_position(widget).cell())
            );
        }
    }
    assert_eq!(state.preferences.weather_location, None);
    assert_eq!(
        state.preferences.temperature_unit,
        crate::weather::Unit::Celsius
    );
    let mut loaded: Preferences =
        serde_json::from_slice(&serde_json::to_vec(&state.preferences).unwrap()).unwrap();
    loaded.sanitize();
    for widget in loaded
        .widgets()
        .into_iter()
        .filter(|widget| loaded.has_widget(*widget))
    {
        assert!(loaded.can_place_widget(widget, loaded.widget_position(widget)));
    }
    let removed = HomeWidget::ExtraCrypto(0);
    state.focus = Target::WidgetCell(state.preferences.widget_position(removed).cell());
    state.activate();
    state.focus = Target::RemoveWidget(removed);
    assert_eq!(state.activate(), [Effect::Save]);
    assert!(!state.preferences.has_widget(removed));
    assert!(state.preferences.has_widget(HomeWidget::Crypto));
    assert!(state.preferences.reset_widgets());
    assert_eq!(
        state
            .preferences
            .widgets()
            .into_iter()
            .filter(|widget| state.preferences.has_widget(*widget))
            .collect::<Vec<_>>(),
        [HomeWidget::Greeting, HomeWidget::Weather]
    );
    assert_eq!(markets::settings(&state.preferences, removed).symbol, "ETH");
}

#[test]
fn move_instructions_are_not_controls_and_remote_can_confirm_and_cancel() {
    let mut state = ready();
    state.open(2);
    state.section = WIDGETS_SECTION;
    let canvas = |state: &State| {
        scene(state)
            .draw
            .into_iter()
            .find_map(|draw| match draw {
                Draw::Image(_, rect, ..) if rect.w == 1120. && rect.h == 630. => {
                    Some((rect.x, rect.y, rect.w, rect.h))
                }
                _ => None,
            })
            .unwrap()
    };
    let original_canvas = canvas(&state);
    let widget = HomeWidget::Weather;
    let saved = state.preferences.widget_position(widget);
    state.focus = Target::MoveWidget(widget);
    state.activate();
    assert_eq!(canvas(&state), original_canvas);
    let rendered = scene(&state);
    for label in ["← ↑ → ↓", "— Move", "OK", "— Confirm", "— Cancel"] {
        let rect = rendered
            .draw
            .iter()
            .find_map(|draw| match draw {
                Draw::Text(value, rect, ..) if value == label => Some(*rect),
                _ => None,
            })
            .unwrap();
        assert!(rect.y + rect.h < original_canvas.1);
        let (x, y) = rect.center();
        assert_eq!(state.hit_test(&rendered.controls, x, y), None);
    }
    assert!(rendered.draw.iter().any(|draw| matches!(
        draw, Draw::TintedImage(path, ..) if path.ends_with("ui-icons/backspace.lvec")
    )));
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.preferences.widget_position(widget), saved);
    state.back();
    assert_eq!(state.preferences.widget_position(widget), saved);
    state.focus = Target::MoveWidget(widget);
    state.activate();
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.activate(), [Effect::Save]);
    assert_eq!(state.preferences.widget_position(widget).row, saved.row + 1);
    assert_eq!(canvas(&state), original_canvas);
}

#[test]
fn unconfigured_markets_show_not_set_and_settings_hint_and_help_is_in_a_card() {
    let mut state = ready();
    state.preferences.crypto.enabled = true;
    state.preferences.stocks.enabled = true;
    let rendered = scene(&state);
    for label in ["Not set", "Configure in Settings"] {
        assert_eq!(
            rendered
                .draw
                .iter()
                .filter(|draw| matches!(draw, Draw::Text(value, ..) if value == label))
                .count(),
            2
        );
    }
    assert!(!rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(value, ..) if value == "Configure" || value == "Change unavailable")));
    state.open(2);
    for language in crate::i18n::Language::ALL {
        state.preferences.language = language;
        for kind in [HomeWidget::Crypto, HomeWidget::Stocks] {
            state.subpage = Some(Subpage::Market(kind));
            let rendered = scene(&state);
            let panel = rendered
                .draw
                .iter()
                .find_map(|draw| match draw {
                    Draw::Glass(rect, ..) if rect.x == 640. && rect.y > 250. => Some(*rect),
                    _ => None,
                })
                .unwrap();
            for draw in &rendered.draw {
                if let Draw::Text(_, rect, ..) = draw
                    && rect.y >= panel.y
                    && rect.x >= panel.x
                {
                    assert!(rect.y + rect.h <= panel.y + panel.h);
                    assert!(rect.x >= panel.x && rect.x + rect.w <= panel.x + panel.w);
                }
            }
            assert!(panel.y + panel.h < 1000.);
        }
    }
}

#[test]
fn optional_market_widgets_add_configure_move_remove_and_preserve_settings_on_reset() {
    for kind in [HomeWidget::Crypto, HomeWidget::Stocks] {
        let mut state = ready();
        assert!(!state.preferences.has_widget(kind));
        state.open(2);
        state.section = WIDGETS_SECTION;
        state.focus = Target::WidgetAdd;
        state.activate();
        assert!(
            controls(&state)
                .iter()
                .any(|control| control.target == Target::AddWidget(kind))
        );
        state.pointer_focus(Target::AddWidget(kind));
        state.activate();
        let anchor = state.preferences.widget_position(kind);
        state.focus = Target::Choice(anchor.cell());
        assert_eq!(state.activate(), [Effect::Save]);
        assert!(state.preferences.has_widget(kind));
        state.focus = Target::WidgetCell(anchor.cell());
        state.activate();
        state.activate();
        assert_eq!(state.subpage, Some(Subpage::Market(kind)));
        assert_eq!(state.focus, Target::MarketSymbol(kind));
        state.activate();
        state.text(if kind == HomeWidget::Crypto {
            " btc "
        } else {
            " sap.de "
        });
        assert_eq!(state.back(), [Effect::Save]);
        let symbol = if kind == HomeWidget::Crypto {
            "BTC"
        } else {
            "SAP.DE"
        };
        assert_eq!(markets::settings(&state.preferences, kind).symbol, symbol);
        if kind == HomeWidget::Crypto {
            state.direction(Direction::Down, &controls(&state));
            assert_eq!(state.focus, Target::MarketCurrency);
            assert_eq!(state.activate(), [Effect::Save]);
            assert_eq!(state.preferences.crypto.currency, "EUR");
        }
        state.back();
        assert_eq!(state.focus, Target::WidgetCell(anchor.cell()));
        state.activate();
        state.focus = Target::MoveWidget(kind);
        state.activate();
        let moved = WidgetPosition { column: 5, row: 3 };
        state.focus = Target::Choice(moved.cell());
        assert_eq!(state.activate(), [Effect::Save]);
        state.activate();
        state.focus = Target::RemoveWidget(kind);
        assert_eq!(state.activate(), [Effect::Save]);
        assert!(!state.preferences.has_widget(kind));
        let mut loaded: Preferences =
            serde_json::from_value(serde_json::to_value(&state.preferences).unwrap()).unwrap();
        loaded.sanitize();
        assert_eq!(markets::settings(&loaded, kind).symbol, symbol);
        loaded.reset_widgets();
        assert!(!loaded.has_widget(kind));
        assert_eq!(markets::settings(&loaded, kind).symbol, symbol);
    }
}

#[test]
fn market_keyboard_handles_native_completion_invalid_symbols_and_unicode_deletion() {
    let mut state = ready();
    state.open(2);
    state.section = WIDGETS_SECTION;
    state.subpage = Some(Subpage::Market(HomeWidget::Stocks));
    state.focus = Target::MarketSymbol(HomeWidget::Stocks);
    state.activate();
    state.text("AAPLx");
    state.delete_surrounding_text(-1, 1);
    state.overlay = Some(Overlay::SystemKeyboard);
    assert_eq!(state.finish_text_input(), [Effect::Save]);
    assert_eq!(state.preferences.stocks.symbol, "AAPL");
    state.activate();
    state.text("bad/symbol");
    assert!(state.back().is_empty());
    assert_eq!(state.preferences.stocks.symbol, "AAPL");
    assert!(state.notice.is_some());
}

#[test]
fn market_defaults_and_corrupt_settings_preserve_existing_preferences() {
    let mut preferences: Preferences =
        serde_json::from_value(json!({"crypto":null,"stocks":{"symbol":42},"hour_24":true}))
            .unwrap();
    preferences.sanitize();
    assert!(preferences.hour_24);
    assert!(!preferences.crypto.enabled && !preferences.stocks.enabled);
    preferences.crypto.enabled = true;
    preferences.stocks.enabled = true;
    preferences.crypto.position = WidgetPosition {
        column: 255,
        row: 255,
    };
    preferences.stocks.position = preferences.greeting_position;
    preferences.sanitize();
    for widget in HomeWidget::ALL {
        assert!(preferences.can_place_widget(widget, preferences.widget_position(widget)));
    }
}

#[test]
fn both_market_widgets_render_readable_price_and_age_without_provider_or_delay_labels() {
    for language in crate::i18n::Language::ALL {
        let mut state = ready();
        let dock = state.dock_y();
        state.preferences.language = language;
        state.preferences.crypto.enabled = true;
        state.preferences.crypto.symbol = "BTC".into();
        state.preferences.stocks.enabled = true;
        state.preferences.stocks.symbol = "AAPL".into();
        for slot in state.markets.slots.iter_mut().take(2) {
            slot.quote = Some(markets::Quote {
                change_percent: Some(4.),
                price: 123.45,
                currency: "USD".into(),
                timestamp: time::OffsetDateTime::now_utc().unix_timestamp() - 60,
            });
        }
        let rendered = scene(&state);
        for expected in ["BTC", "AAPL", "123.45 USD"] {
            assert!(
                rendered
                    .draw
                    .iter()
                    .any(|draw| matches!(draw, Draw::Text(label, ..) if label == expected))
            );
        }
        assert!(!rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains(language.t("marketDelayed")))));
        assert!(
            rendered
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label == &language.format("marketMinutesAgo", &[("count", "1")])))
        );
        assert!(!rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains("Coinbase") || label.contains("Yahoo Finance"))));
        assert_eq!(state.dock_y(), dock);
    }
}

#[test]
fn market_trends_have_matching_colored_arrows_and_neutral_zeroes_have_no_arrow() {
    let mut state = ready();
    state.preferences.crypto.enabled = true;
    state.preferences.crypto.symbol = "BTC".into();
    state.preferences.stocks.enabled = true;
    state.preferences.stocks.symbol = "AAPL".into();
    for (index, change) in [3.25, -2.5].into_iter().enumerate() {
        state.markets.slots[index].quote = Some(markets::Quote {
            price: 100.,
            currency: "USD".into(),
            timestamp: time::OffsetDateTime::now_utc().unix_timestamp() - 60,
            change_percent: Some(change),
        });
    }
    let rendered = scene(&state);
    let percentage_rect = rendered
        .draw
        .iter()
        .find_map(|draw| match draw {
            Draw::Text(label, rect, ..) if label == "+3.25%" => Some(*rect),
            _ => None,
        })
        .unwrap();
    let age_rect = rendered
        .draw
        .iter()
        .find_map(|draw| match draw {
            Draw::Text(label, rect, ..) if label == "1m ago" => Some(*rect),
            _ => None,
        })
        .unwrap();
    assert!(age_rect.x > percentage_rect.x + 16.);
    assert!(
        age_rect.x < percentage_rect.x + 130.,
        "Age should follow the percentage, not the widget's right edge"
    );
    assert!((age_rect.y - percentage_rect.y).abs() <= 3.);
    assert!(rendered.draw.iter().any(|draw| matches!(draw, Draw::TintedImage(path, _, _, _, color) if path.ends_with("market-up.lvec") && color[1] > color[0])));
    assert!(rendered.draw.iter().any(|draw| matches!(draw, Draw::TintedImage(path, _, _, _, color) if path.ends_with("market-down.lvec") && color[0] > color[1])));
    assert!(rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, _, _, color, _) if label == "+3.25%" && color[1] > color[0])));
    assert!(rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, _, _, color, _) if label == "-2.50%" && color[0] > color[1])));
    for change in [Some(0.), Some(-0.0001), None] {
        for slot in state.markets.slots.iter_mut().take(2) {
            slot.quote.as_mut().unwrap().change_percent = change;
        }
        let rendered = scene(&state);
        assert!(!rendered.draw.iter().any(|draw| matches!(draw, Draw::TintedImage(path, ..) if path.ends_with("market-up.lvec") || path.ends_with("market-down.lvec"))));
        let expected = if change.is_some() {
            "0.00%"
        } else {
            "Change unavailable"
        };
        assert!(
            rendered
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label == expected))
        );
    }
}
