use super::*;
#[test]
fn crypto_coins_are_chosen_in_widgets_up_to_four() {
    let mut state = ready();
    state.open(2);
    state.focus = Target::Widget(5);
    state.activate();
    assert_eq!(state.subpage, Some(crate::state::Subpage::Crypto));
    assert_eq!(state.focus, Target::CryptoToggle);
    assert_eq!(state.activate(), vec![Effect::Save]);
    assert!(state.preferences.show_crypto);
    // BTC, ETH and SOL are chosen by default; BNB makes four.
    state.focus = Target::CryptoCoin(3);
    assert_eq!(state.activate(), vec![Effect::Save]);
    state.focus = Target::CryptoCoin(7);
    assert!(state.activate().is_empty(), "a fifth coin is refused");
    assert!(state.notice.is_some());
    assert_eq!(state.preferences.crypto_coins, ["BTC", "ETH", "SOL", "BNB"]);
    state.focus = Target::CryptoCoin(0);
    state.activate();
    state.focus = Target::CryptoCoin(7);
    state.activate();
    assert_eq!(state.preferences.crypto_coins, ["ETH", "SOL", "BNB", "TON"]);
    assert!(
        controls(&state)
            .iter()
            .any(|c| c.target == Target::CryptoCoin(7))
    );
    state.direction(Direction::Left, &controls(&state));
    assert_eq!(state.focus, Target::Widget(5));
}

#[test]
fn crypto_cards_follow_inputs_and_open_their_settings() {
    let mut state = ready();
    let positions = |scene: &view::Scene| -> Vec<_> {
        scene
            .controls
            .iter()
            .filter(|c| matches!(c.target, Target::App(_) | Target::Input(_)))
            .map(|c| (c.target.clone(), c.rect.x, c.rect.y))
            .collect()
    };
    let before = scene(&state);
    state.preferences.show_crypto = true;
    let (Some(command), _) =
        state
            .crypto
            .sync(true, &state.preferences.crypto_coins.clone(), true, 1000)
    else {
        panic!();
    };
    let quote = |symbol: &str, price, change| crate::crypto::Quote {
        symbol: symbol.into(),
        price,
        change,
    };
    state.crypto.accept(
        command.id,
        Ok(vec![
            quote("BTC", 67420.12, 1.23),
            quote("ETH", 2600.5, -3.69),
            quote("SOL", 0.5, 0.),
        ]),
        1000,
    );
    let after = scene(&state);
    assert_eq!(positions(&before), positions(&after));
    let text = drawn_text(&after);
    for expected in [
        "Crypto",
        "BTC",
        "Bitcoin",
        "$67,420.12",
        "+1.23%",
        "−3.69%",
        "$0.5000",
    ] {
        assert!(text.iter().any(|t| t == expected), "{expected} in {text:?}");
    }
    let input_bottom = after
        .controls
        .iter()
        .filter(|c| matches!(c.target, Target::Input(_)))
        .map(|c| c.rect.y + c.rect.h)
        .fold(0., f32::max);
    let coins: Vec<_> = after
        .controls
        .iter()
        .filter(|c| matches!(c.target, Target::Coin(_)))
        .collect();
    assert_eq!(coins.len(), 3);
    assert!(coins.iter().all(|c| c.rect.y > input_bottom));

    // Down from the inputs reaches the cards and scrolls them into view.
    state.focus = Target::Input("com.webos.app.hdmi1".into());
    state.direction(Direction::Down, &controls(&state));
    assert_eq!(state.focus, Target::Coin(0));
    let card = controls(&state)
        .into_iter()
        .find(|c| c.target == Target::Coin(0))
        .unwrap();
    assert!(card.rect.y + card.rect.h <= 1080.);
    state.activate();
    assert_eq!(state.subpage, Some(crate::state::Subpage::Crypto));
    assert_eq!(state.focus, Target::CryptoToggle);
}

fn drawn_text(scene: &view::Scene) -> Vec<String> {
    scene
        .draw
        .iter()
        .filter_map(|draw| match draw {
            crate::renderer::Draw::Text(text, ..) => Some(text.clone()),
            _ => None,
        })
        .collect()
}
