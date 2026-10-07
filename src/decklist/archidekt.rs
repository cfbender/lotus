//! Archidekt's public deck API
//! (`Manavault.Trade.ListSource.Archidekt`, `TheGathering.Decklists.Sources.Archidekt`).
//!
//! Archidekt has no boards, only categories: a deck declares which
//! categories are part of the deck (`includedInDeck`), and a card lives
//! where its first (primary) category says. `Commander` is a category too.

use std::collections::HashSet;

use serde::Deserialize;

use crate::card::{Color, Finish, Zone};
use crate::decklist::{Decklist, Entry, Source};

/// The API base; the deck id and a trailing slash are appended.
pub const API_BASE: &str = "https://archidekt.com/api/decks/";

/// The API URL for a validated deck id.
#[must_use]
pub fn api_url(api_base: &str, id: &str) -> String {
    format!("{api_base}{id}/")
}

/// Categories treated as outside the deck when the payload carries no
/// category metadata at all.
const DEFAULT_EXCLUDED: [&str; 2] = ["Maybeboard", "Sideboard"];

/// The subset of Archidekt's deck payload both apps read.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchidektDeck {
    /// The deck name.
    #[serde(default)]
    pub name: Option<String>,
    /// The deck's owner.
    #[serde(default)]
    pub owner: Option<ArchidektOwner>,
    /// The deck's categories and whether each is part of the deck. Absent in
    /// older payloads, in which case `Maybeboard` and `Sideboard` are
    /// treated as excluded.
    #[serde(default)]
    pub categories: Option<Vec<ArchidektCategory>>,
    /// Every card in the deck, including excluded categories.
    #[serde(default)]
    pub cards: Vec<ArchidektEntry>,
}

/// An Archidekt user.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ArchidektOwner {
    /// The login name.
    #[serde(default)]
    pub username: Option<String>,
}

/// A category declaration.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchidektCategory {
    /// The category name.
    pub name: String,
    /// Whether cards whose primary category this is count as deck contents.
    #[serde(default = "default_true")]
    pub included_in_deck: bool,
}

fn default_true() -> bool {
    true
}

/// One card in the deck.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ArchidektEntry {
    /// How many copies.
    #[serde(default)]
    pub quantity: Option<i64>,
    /// The entry's categories; the first is primary.
    #[serde(default)]
    pub categories: Vec<String>,
    /// `Normal`, `Foil`, or `Etched`.
    #[serde(default)]
    pub modifier: Option<String>,
    /// The printing.
    #[serde(default)]
    pub card: Option<ArchidektCard>,
}

/// A printing.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchidektCard {
    /// The Scryfall printing id.
    #[serde(default)]
    pub uid: Option<String>,
    /// The Oracle card.
    #[serde(default)]
    pub oracle_card: Option<ArchidektOracleCard>,
}

/// An Oracle card.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchidektOracleCard {
    /// The card name.
    #[serde(default)]
    pub name: Option<String>,
    /// Color identity as full color names (`White`), or codes.
    #[serde(default)]
    pub color_identity: Option<Vec<String>>,
}

impl ArchidektEntry {
    /// The finish from the `modifier`, defaulting to non-foil.
    #[must_use]
    pub fn finish(&self) -> Finish {
        match self.modifier.as_deref() {
            Some("Foil") => Finish::Foil,
            Some("Etched") => Finish::Etched,
            _ => Finish::Nonfoil,
        }
    }

    /// The zone: considering when the primary category is excluded from
    /// the deck, the command zone when `Commander` is among the categories,
    /// else the main deck.
    #[must_use]
    pub fn zone(&self, excluded: &HashSet<&str>) -> Zone {
        if self
            .categories
            .first()
            .is_some_and(|primary| excluded.contains(primary.as_str()))
        {
            Zone::Considering
        } else if self
            .categories
            .iter()
            .any(|category| category == "Commander")
        {
            Zone::Commander
        } else {
            Zone::Mainboard
        }
    }

    fn oracle_card(&self) -> Option<&ArchidektOracleCard> {
        self.card.as_ref()?.oracle_card.as_ref()
    }

    fn entry(&self, zone: Zone) -> Option<Entry> {
        let mut entry = Entry::from_source(
            self.oracle_card()?.name.clone(),
            self.quantity,
            zone,
            self.card.as_ref().and_then(|card| card.uid.clone()),
        )?;
        entry.finish = self.finish();
        Some(entry)
    }
}

impl ArchidektDeck {
    /// Decodes an API response body.
    pub fn parse(json: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(json)
    }

    /// Names of categories whose cards are not deck contents.
    #[must_use]
    pub fn excluded_categories(&self) -> HashSet<&str> {
        match &self.categories {
            Some(categories) => categories
                .iter()
                .filter(|category| !category.included_in_deck)
                .map(|category| category.name.as_str())
                .collect(),
            None => DEFAULT_EXCLUDED.into_iter().collect(),
        }
    }

    /// The deck as a [`Decklist`] for the validated `id`, in Archidekt's
    /// card order. `card_count` sums the quantities in the command zone and
    /// main deck; `color_identity` is the union of the commanders'.
    #[must_use]
    pub fn into_decklist(self, id: &str) -> Decklist {
        let excluded = self.excluded_categories();
        let mut entries = Vec::with_capacity(self.cards.len());
        let mut commanders = Vec::new();
        let mut colors = Vec::new();
        for card in &self.cards {
            let zone = card.zone(&excluded);
            let Some(entry) = card.entry(zone) else {
                continue;
            };
            if zone == Zone::Commander {
                commanders.push(entry.name.clone());
                colors.extend(
                    card.oracle_card()
                        .and_then(|oracle| oracle.color_identity.as_ref())
                        .into_iter()
                        .flatten()
                        .filter_map(|name| Color::parse(name)),
                );
            }
            entries.push(entry);
        }
        let card_count = entries
            .iter()
            .filter(|entry| entry.zone.in_deck())
            .map(|entry| u64::from(entry.quantity.get()))
            .sum();
        Decklist {
            source: Source::Archidekt,
            id: id.to_owned(),
            url: format!("https://archidekt.com/decks/{id}"),
            name: self.name,
            author: self.owner.and_then(|owner| owner.username),
            card_count: Some(card_count),
            commanders,
            color_identity: Color::identity(colors),
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(json: serde_json::Value) -> ArchidektEntry {
        serde_json::from_value(json).unwrap()
    }

    #[test]
    fn zone_follows_primary_category_then_commander() {
        let excluded: HashSet<&str> = ["Maybeboard"].into_iter().collect();
        assert_eq!(
            entry(serde_json::json!({"categories": ["Commander"]})).zone(&excluded),
            Zone::Commander
        );
        assert_eq!(
            entry(serde_json::json!({"categories": ["Maybeboard", "Commander"]})).zone(&excluded),
            Zone::Considering,
            "an excluded primary category wins over a secondary Commander tag"
        );
        assert_eq!(
            entry(serde_json::json!({"categories": ["Ramp", "Maybeboard"]})).zone(&excluded),
            Zone::Mainboard,
            "only the primary category decides inclusion"
        );
        assert_eq!(
            entry(serde_json::json!({})).zone(&excluded),
            Zone::Mainboard
        );
    }

    #[test]
    fn missing_category_metadata_falls_back_to_default_boards() {
        let deck = ArchidektDeck::parse(
            br#"{"cards": [
              {"quantity": 1, "categories": ["Sideboard"], "card": {"uid": "u1", "oracleCard": {"name": "A"}}},
              {"quantity": 1, "categories": ["Custom"], "card": {"uid": "u2", "oracleCard": {"name": "B"}}}
            ]}"#,
        )
        .unwrap()
        .into_decklist("1");
        let zones: Vec<Zone> = deck.entries.iter().map(|entry| entry.zone).collect();
        assert_eq!(zones, vec![Zone::Considering, Zone::Mainboard]);
        assert_eq!(deck.card_count, Some(1));
    }

    #[test]
    fn custom_excluded_categories_and_finishes() {
        let deck = ArchidektDeck::parse(
            br#"{
              "name": "Custom",
              "owner": {"username": "someone"},
              "categories": [
                {"name": "Commander", "includedInDeck": true},
                {"name": "Maybeboard", "includedInDeck": false},
                {"name": "Cut", "includedInDeck": false},
                {"name": "Sideboard", "includedInDeck": true}
              ],
              "cards": [
                {"quantity": 1, "categories": ["Commander"], "modifier": "Foil", "card": {"uid": "u1", "oracleCard": {"name": "Atraxa, Praetors' Voice", "colorIdentity": ["Green", "White", "Blue", "Black"]}}},
                {"quantity": 2, "categories": ["Cut"], "modifier": "Etched", "card": {"uid": "u2", "oracleCard": {"name": "Cultivate"}}},
                {"quantity": 3, "categories": ["Sideboard"], "modifier": "Normal", "card": {"uid": "u3", "oracleCard": {"name": "Island"}}},
                {"quantity": 1, "categories": ["Mainboard"], "card": {"uid": "u4", "oracleCard": {"name": ""}}},
                {"quantity": 1, "categories": ["Mainboard"], "card": {"uid": "u5"}}
              ]
            }"#,
        )
        .unwrap()
        .into_decklist("42");
        assert_eq!(deck.author.as_deref(), Some("someone"));
        assert_eq!(deck.commanders, vec!["Atraxa, Praetors' Voice"]);
        assert_eq!(
            deck.color_identity,
            vec![Color::W, Color::U, Color::B, Color::G]
        );
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
                ("Atraxa, Praetors' Voice", 1, Zone::Commander, Finish::Foil),
                ("Cultivate", 2, Zone::Considering, Finish::Etched),
                ("Island", 3, Zone::Mainboard, Finish::Nonfoil),
            ]
        );
        assert_eq!(
            deck.card_count,
            Some(4),
            "a sideboard marked as included counts"
        );
        assert_eq!(
            api_url(API_BASE, "42"),
            "https://archidekt.com/api/decks/42/"
        );
    }
}
