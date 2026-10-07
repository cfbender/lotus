//! The decklist sources against the-gathering's recorded fixtures
//! (`test/support/fixtures/decklists`), asserting what its
//! `decklists_test.exs` asserts plus what ManaVault's list sources add.

use lotus::card::{Color, Finish, Zone};
use lotus::decklist::manavault::{DeckData, DeckPager, GraphqlResponse, Limits, Step};
use lotus::decklist::{ArchidektDeck, Decklist, MoxfieldDeck, Source};

const MOXFIELD_PARTNER: &[u8] = include_bytes!("fixtures/decklists/moxfield_partner.json");
const ARCHIDEKT_BACKGROUND: &[u8] = include_bytes!("fixtures/decklists/archidekt_background.json");
const MANAVAULT: &[u8] = include_bytes!("fixtures/decklists/manavault.json");

fn summary(deck: &Decklist) -> Vec<(&str, u32, Zone, Option<&str>)> {
    deck.entries
        .iter()
        .map(|entry| {
            (
                entry.name.as_str(),
                entry.quantity.get(),
                entry.zone,
                entry.scryfall_id.as_ref().map(lotus::ScryfallId::as_str),
            )
        })
        .collect()
}

#[test]
fn moxfield_deck_with_partner_commanders() {
    let deck = MoxfieldDeck::parse(MOXFIELD_PARTNER)
        .expect("parses")
        .into_decklist("partners");

    assert_eq!(deck.source, Source::Moxfield);
    assert_eq!(deck.url, "https://moxfield.com/decks/partners");
    assert_eq!(deck.name.as_deref(), Some("Partner Commander"));
    assert_eq!(
        deck.commanders,
        vec!["Kraum, Ludevic's Opus", "Malcolm, Keen-Eyed Navigator"]
    );
    assert_eq!(deck.color_identity, vec![Color::U, Color::R]);
    assert_eq!(deck.author.as_deref(), Some("Goodybarsco"));
    assert_eq!(deck.card_count, Some(100));
    assert_eq!(
        summary(&deck),
        vec![
            (
                "Kraum, Ludevic's Opus",
                1,
                Zone::Commander,
                Some("5b4d8b79-7a17-4f07-9dd5-4bb3ee0d3a5d")
            ),
            (
                "Malcolm, Keen-Eyed Navigator",
                1,
                Zone::Commander,
                Some("9d5b2c1e-3e77-4a4f-9d52-1b4a3c8e6f10")
            ),
            (
                "Island",
                12,
                Zone::Mainboard,
                Some("a1b2c3d4-0000-4000-8000-000000000001")
            ),
            (
                "Sol Ring",
                1,
                Zone::Mainboard,
                Some("7e0c2f04-1d50-4fcd-9f1c-3c2a1b0e9d8f")
            ),
            ("Counterspell", 1, Zone::Considering, None),
        ]
    );
    assert!(
        deck.entries
            .iter()
            .all(|entry| entry.finish == Finish::Nonfoil)
    );
    let playable: Vec<&str> = deck.playable().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        playable,
        vec![
            "Kraum, Ludevic's Opus",
            "Malcolm, Keen-Eyed Navigator",
            "Island",
            "Sol Ring"
        ]
    );
}

#[test]
fn archidekt_deck_with_a_background() {
    let deck = ArchidektDeck::parse(ARCHIDEKT_BACKGROUND)
        .expect("parses")
        .into_decklist("24907541");

    assert_eq!(deck.source, Source::Archidekt);
    assert_eq!(deck.url, "https://archidekt.com/decks/24907541");
    assert_eq!(deck.name.as_deref(), Some("I am a public servant"));
    assert_eq!(
        deck.commanders,
        vec!["Noble Heritage", "Wilson, Refined Grizzly"]
    );
    assert_eq!(deck.color_identity, vec![Color::W, Color::G]);
    assert_eq!(deck.author.as_deref(), Some("Will3545"));
    // The Maybeboard is excluded from the deck, so the count leaves out Cultivate.
    assert_eq!(deck.card_count, Some(100));
    assert_eq!(
        summary(&deck),
        vec![
            (
                "Noble Heritage",
                1,
                Zone::Commander,
                Some("0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e01")
            ),
            (
                "Wilson, Refined Grizzly",
                1,
                Zone::Commander,
                Some("0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e02")
            ),
            (
                "Forest",
                38,
                Zone::Mainboard,
                Some("0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e03")
            ),
            ("Other cards", 60, Zone::Mainboard, None),
            (
                "Cultivate",
                1,
                Zone::Considering,
                Some("0c4b3e5a-8f5d-4a32-9f6e-2b1d7c9a4e05")
            ),
        ]
    );
}

#[test]
fn manavault_shared_deck() {
    let response: GraphqlResponse<DeckData> = serde_json::from_slice(MANAVAULT).expect("parses");
    let mut pager = DeckPager::new("AbCdEfGhIjKlMnOpQrStUvWx", Limits::default());
    assert_eq!(pager.accept(response, 1024), Ok(Step::Done));
    let deck = pager.finish("https://manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx");

    assert_eq!(deck.source, Source::ManaVault);
    assert_eq!(deck.id, "AbCdEfGhIjKlMnOpQrStUvWx");
    assert_eq!(deck.name.as_deref(), Some("Shared Deck"));
    assert_eq!(deck.commanders, vec!["Shorikai, Genesis Engine"]);
    assert_eq!(deck.color_identity, vec![Color::W, Color::U]);
    assert_eq!(deck.author, None);
    assert_eq!(deck.card_count, Some(100));
    // The preferred printing wins over the fallback; `considering` stays out of the playable list.
    assert_eq!(
        summary(&deck),
        vec![
            (
                "Shorikai, Genesis Engine",
                1,
                Zone::Commander,
                Some("b3a0e8d4-1f2c-4c4e-9a55-6f1d2e3c4b01")
            ),
            (
                "Sol Ring",
                1,
                Zone::Mainboard,
                Some("b3a0e8d4-1f2c-4c4e-9a55-6f1d2e3c4b02")
            ),
            ("Rhystic Study", 1, Zone::Considering, None),
        ]
    );
    assert_eq!(deck.playable().count(), 2);
}
