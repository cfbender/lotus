//! Moxfield's unofficial v3 deck API
//! (`Manavault.Trade.ListSource.Moxfield`, `TheGathering.Decklists.Sources.Moxfield`).
//!
//! Only `api2.moxfield.com` is ever requested, and only with an id that
//! already matched [`DeckLink`](crate::decklist::DeckLink)'s pattern.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::card::{Color, Finish, Zone};
use crate::decklist::{Decklist, Entry, Source};

/// The API base; the deck id is appended.
pub const API_BASE: &str = "https://api2.moxfield.com/v3/decks/all/";

/// The API URL for a validated deck id.
#[must_use]
pub fn api_url(api_base: &str, id: &str) -> String {
    format!("{api_base}{id}")
}

/// The subset of Moxfield's deck payload both apps read.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoxfieldDeck {
    /// The deck name.
    #[serde(default)]
    pub name: Option<String>,
    /// The deck's owner.
    #[serde(default)]
    pub created_by_user: Option<MoxfieldUser>,
    /// Boards keyed by name: `commanders`, `mainboard`, `sideboard`,
    /// `maybeboard`, and others this crate ignores.
    #[serde(default)]
    pub boards: BTreeMap<String, MoxfieldBoard>,
}

/// A Moxfield user.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoxfieldUser {
    /// The display name, preferred as the author.
    #[serde(default)]
    pub display_name: Option<String>,
    /// The login name, used when there is no display name.
    #[serde(default)]
    pub user_name: Option<String>,
}

/// One board of a deck.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoxfieldBoard {
    /// Moxfield's own count of cards on the board.
    #[serde(default)]
    pub count: Option<u64>,
    /// Entries keyed by Moxfield's card id.
    #[serde(default)]
    pub cards: BTreeMap<String, MoxfieldEntry>,
}

/// One card on a board.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoxfieldEntry {
    /// How many copies.
    #[serde(default)]
    pub quantity: Option<i64>,
    /// `nonFoil`, `foil`, or `etched`.
    #[serde(default)]
    pub finish: Option<String>,
    /// Older payloads flag foils here instead of `finish`.
    #[serde(default)]
    pub is_foil: Option<bool>,
    /// The printing.
    #[serde(default)]
    pub card: Option<MoxfieldCard>,
}

/// The printing an entry names.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct MoxfieldCard {
    /// The card name.
    #[serde(default)]
    pub name: Option<String>,
    /// The set code.
    #[serde(default)]
    pub set: Option<String>,
    /// The collector number.
    #[serde(default)]
    pub cn: Option<String>,
    /// The Scryfall printing id.
    #[serde(default)]
    pub scryfall_id: Option<String>,
    /// Color identity as single-letter codes.
    #[serde(default)]
    pub color_identity: Option<Vec<String>>,
}

impl MoxfieldEntry {
    /// The finish: `finish` when it is a known value, else `isFoil`, else
    /// non-foil.
    #[must_use]
    pub fn finish(&self) -> Finish {
        match self.finish.as_deref() {
            Some("nonFoil") => Finish::Nonfoil,
            Some("foil") => Finish::Foil,
            Some("etched") => Finish::Etched,
            _ if self.is_foil == Some(true) => Finish::Foil,
            _ => Finish::Nonfoil,
        }
    }

    fn entry(&self, zone: Zone) -> Option<Entry> {
        let card = self.card.as_ref()?;
        let mut entry = Entry::from_source(
            card.name.clone(),
            self.quantity,
            zone,
            card.scryfall_id.clone(),
        )?;
        entry.set_code = card.set.clone().filter(|set| !set.is_empty());
        entry.collector_number = card.cn.clone().filter(|cn| !cn.is_empty());
        entry.finish = self.finish();
        Some(entry)
    }
}

/// Which boards become which zone. Boards not listed (tokens, attractions,
/// stickers, planes) are ignored.
const BOARDS: [(&str, Zone); 4] = [
    ("commanders", Zone::Commander),
    ("mainboard", Zone::Mainboard),
    ("sideboard", Zone::Considering),
    ("maybeboard", Zone::Considering),
];

impl MoxfieldDeck {
    /// Decodes an API response body.
    pub fn parse(json: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(json)
    }

    /// The entries of a board, sorted by name. Entries without a card name
    /// are dropped.
    #[must_use]
    pub fn board_entries(&self, board: &str, zone: Zone) -> Vec<Entry> {
        let mut entries: Vec<Entry> = self
            .boards
            .get(board)
            .map(|board| {
                board
                    .cards
                    .values()
                    .filter_map(|entry| entry.entry(zone))
                    .collect()
            })
            .unwrap_or_default();
        entries.sort_by(|a, b| a.name.cmp(&b.name));
        entries
    }

    fn board_count(&self, board: &str) -> u64 {
        self.boards
            .get(board)
            .and_then(|board| board.count)
            .unwrap_or(0)
    }

    /// The deck as a [`Decklist`] for the validated `id`.
    ///
    /// Commanders come first, then the main deck, then the sideboard and
    /// maybeboard as [`Zone::Considering`]; each board is sorted by name.
    /// `card_count` is Moxfield's main deck count plus its commander count,
    /// and `color_identity` is the union of the commanders' identities.
    #[must_use]
    pub fn into_decklist(self, id: &str) -> Decklist {
        let commanders = self.board_entries("commanders", Zone::Commander);
        let color_identity = Color::identity(
            self.boards
                .get("commanders")
                .into_iter()
                .flat_map(|board| board.cards.values())
                .filter_map(|entry| entry.card.as_ref())
                .flat_map(|card| card.color_identity.iter().flatten())
                .filter_map(|code| Color::parse(code)),
        );
        let card_count = self.board_count("mainboard") + self.board_count("commanders");
        let mut entries = commanders;
        for (board, zone) in BOARDS.iter().skip(1) {
            entries.extend(self.board_entries(board, *zone));
        }
        let author = self.created_by_user.and_then(|user| {
            user.display_name
                .filter(|name| !name.is_empty())
                .or(user.user_name)
        });
        Decklist {
            source: Source::Moxfield,
            id: id.to_owned(),
            url: format!("https://moxfield.com/decks/{id}"),
            name: self.name,
            author,
            card_count: Some(card_count),
            commanders: entries
                .iter()
                .filter(|entry| entry.zone == Zone::Commander)
                .map(|entry| entry.name.clone())
                .collect(),
            color_identity,
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::Quantity;

    fn entry(json: serde_json::Value) -> MoxfieldEntry {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn finish_prefers_finish_then_is_foil() {
        assert_eq!(
            entry(serde_json::json!({"finish": "etched"})).finish(),
            Finish::Etched
        );
        assert_eq!(
            entry(serde_json::json!({"finish": "nonFoil", "isFoil": true})).finish(),
            Finish::Nonfoil
        );
        assert_eq!(
            entry(serde_json::json!({"finish": "weird", "isFoil": true})).finish(),
            Finish::Foil
        );
        assert_eq!(
            entry(serde_json::json!({"isFoil": false})).finish(),
            Finish::Nonfoil
        );
        assert_eq!(entry(serde_json::json!({})).finish(), Finish::Nonfoil);
    }

    #[test]
    fn boards_map_to_zones_and_sort_by_name() {
        let deck = MoxfieldDeck::parse(
            br#"{
              "name": "Test",
              "createdByUser": {"displayName": "", "userName": "login"},
              "boards": {
                "commanders": {"count": 1, "cards": {"a": {"quantity": 1, "card": {"name": "Zur the Enchanter", "scryfall_id": "sid-1", "set": "csp", "cn": "41", "color_identity": ["W", "U", "B"]}}}},
                "mainboard": {"count": 3, "cards": {
                  "b": {"quantity": 2, "finish": "foil", "card": {"name": "Sol Ring", "set": "", "cn": ""}},
                  "c": {"quantity": 1, "card": {"name": "Arcane Signet"}},
                  "d": {"quantity": 0, "card": {"name": ""}}
                }},
                "sideboard": {"count": 1, "cards": {"e": {"quantity": 1, "card": {"name": "Counterspell"}}}},
                "maybeboard": {"count": 1, "cards": {"f": {"quantity": -3, "card": {"name": "Brainstorm"}}}},
                "tokens": {"count": 1, "cards": {"g": {"quantity": 1, "card": {"name": "Soldier"}}}}
              }
            }"#,
        )
        .unwrap()
        .into_decklist("abcde");

        assert_eq!(deck.source, Source::Moxfield);
        assert_eq!(deck.url, "https://moxfield.com/decks/abcde");
        assert_eq!(deck.name.as_deref(), Some("Test"));
        assert_eq!(
            deck.author.as_deref(),
            Some("login"),
            "blank display name falls back"
        );
        assert_eq!(deck.card_count, Some(4));
        assert_eq!(deck.commanders, vec!["Zur the Enchanter"]);
        assert_eq!(deck.color_identity, vec![Color::W, Color::U, Color::B]);
        let summary: Vec<(&str, u32, Zone, Finish)> = deck
            .entries
            .iter()
            .map(|entry| {
                (
                    entry.name.as_str(),
                    entry.quantity.get(),
                    entry.zone,
                    entry.finish,
                )
            })
            .collect();
        assert_eq!(
            summary,
            vec![
                ("Zur the Enchanter", 1, Zone::Commander, Finish::Nonfoil),
                ("Arcane Signet", 1, Zone::Mainboard, Finish::Nonfoil),
                ("Sol Ring", 2, Zone::Mainboard, Finish::Foil),
                ("Counterspell", 1, Zone::Considering, Finish::Nonfoil),
                ("Brainstorm", 1, Zone::Considering, Finish::Nonfoil),
            ]
        );
        let zur = &deck.entries[0];
        assert_eq!(
            zur.scryfall_id.as_ref().map(ToString::to_string).as_deref(),
            Some("sid-1")
        );
        assert_eq!(zur.set_code.as_deref(), Some("csp"));
        assert_eq!(zur.collector_number.as_deref(), Some("41"));
        let sol_ring = &deck.entries[2];
        assert_eq!(sol_ring.set_code, None, "blank set codes are dropped");
        assert_eq!(sol_ring.collector_number, None);
        assert_eq!(
            deck.entries[4].quantity,
            Quantity::ONE,
            "negative quantities become one"
        );
    }

    #[test]
    fn empty_payload_is_an_empty_deck() {
        let deck = MoxfieldDeck::parse(b"{}").unwrap().into_decklist("abcde");
        assert_eq!(deck.card_count, Some(0));
        assert!(deck.entries.is_empty());
        assert!(deck.color_identity.is_empty());
        assert_eq!(deck.author, None);
        assert_eq!(
            api_url(API_BASE, "abcde"),
            "https://api2.moxfield.com/v3/decks/all/abcde"
        );
    }
}
