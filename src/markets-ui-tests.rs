use super::*;
use crate::{
    markets,
    model::{HomeWidget, WidgetPosition},
    renderer::Draw,
    state::{Subpage, WIDGETS_SECTION},
};

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
fn both_market_widgets_render_readable_price_source_timestamp_and_delay_without_changing_dock() {
    for language in crate::i18n::Language::ALL {
        let mut state = ready();
        let dock = state.dock_y();
        state.preferences.language = language;
        state.preferences.crypto.enabled = true;
        state.preferences.crypto.symbol = "BTC".into();
        state.preferences.stocks.enabled = true;
        state.preferences.stocks.symbol = "AAPL".into();
        for slot in &mut state.markets.slots {
            slot.quote = Some(markets::Quote {
                price: 123.45,
                currency: "USD".into(),
                timestamp: 1794862801,
            });
        }
        let rendered = scene(&state);
        for expected in ["BTC", "AAPL", "123.45 USD", "Coinbase"] {
            assert!(
                rendered
                    .draw
                    .iter()
                    .any(|draw| matches!(draw, Draw::Text(label, ..) if label == expected))
            );
        }
        assert!(rendered.draw.iter().any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains(language.t("marketDelayed")))));
        assert!(
            rendered
                .draw
                .iter()
                .any(|draw| matches!(draw, Draw::Text(label, ..) if label.contains("UTC")))
        );
        assert_eq!(state.dock_y(), dock);
    }
}
