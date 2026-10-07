//! Card-level vocabulary: finishes, conditions, colors, rarities, and the
//! type-line and layout predicates both apps rely on.
//!
//! Text values match what the Elixir apps store today (`snake_case`), so a
//! Rust backend can read the existing SQLite columns.

use std::fmt;

use serde::{Deserialize, Serialize};

/// The surface treatment of a physical card, as Scryfall reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
pub enum Finish {
    /// A regular, non-foil card.
    Nonfoil,
    /// Traditional foil.
    Foil,
    /// Etched foil.
    Etched,
}

impl Finish {
    /// The stored text value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Nonfoil => "nonfoil",
            Self::Foil => "foil",
            Self::Etched => "etched",
        }
    }

    /// Parses a stored text value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "nonfoil" => Some(Self::Nonfoil),
            "foil" => Some(Self::Foil),
            "etched" => Some(Self::Etched),
            _ => None,
        }
    }

    /// The finish used when a source does not say: [`Finish::Nonfoil`].
    pub const DEFAULT: Self = Self::Nonfoil;

    /// The first finish a printing offers, preferring `current` when it is
    /// still available, and falling back to [`Finish::Nonfoil`] when the
    /// printing lists none (`Manavault.Catalog.Finishes.preferred/2`).
    #[must_use]
    pub fn preferred(available: &[Finish], current: Option<Finish>) -> Finish {
        match current {
            Some(finish) if available.contains(&finish) => finish,
            _ => available.first().copied().unwrap_or(Self::DEFAULT),
        }
    }
}

impl Default for Finish {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl fmt::Display for Finish {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Physical card condition, using the grading scale common to US vendors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
pub enum Condition {
    /// Near mint.
    NearMint,
    /// Lightly played.
    LightlyPlayed,
    /// Moderately played.
    ModeratelyPlayed,
    /// Heavily played.
    HeavilyPlayed,
    /// Damaged.
    Damaged,
}

impl Condition {
    /// The stored text value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NearMint => "near_mint",
            Self::LightlyPlayed => "lightly_played",
            Self::ModeratelyPlayed => "moderately_played",
            Self::HeavilyPlayed => "heavily_played",
            Self::Damaged => "damaged",
        }
    }

    /// Parses a stored text value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "near_mint" => Some(Self::NearMint),
            "lightly_played" => Some(Self::LightlyPlayed),
            "moderately_played" => Some(Self::ModeratelyPlayed),
            "heavily_played" => Some(Self::HeavilyPlayed),
            "damaged" => Some(Self::Damaged),
            _ => None,
        }
    }
}

impl fmt::Display for Condition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One of the five colors of Magic, in WUBRG order.
///
/// Serializes as Scryfall's single-letter code. Derived ordering is WUBRG,
/// which is also the order both apps display color identities in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Color {
    /// White.
    W,
    /// Blue.
    U,
    /// Black.
    B,
    /// Red.
    R,
    /// Green.
    G,
}

impl Color {
    /// Every color in WUBRG order.
    pub const ALL: [Color; 5] = [Color::W, Color::U, Color::B, Color::R, Color::G];

    /// Scryfall's single-letter code.
    #[must_use]
    pub fn code(self) -> &'static str {
        match self {
            Self::W => "W",
            Self::U => "U",
            Self::B => "B",
            Self::R => "R",
            Self::G => "G",
        }
    }

    /// Parses either a single-letter code (`"W"`) or Archidekt's full name
    /// (`"White"`).
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "W" | "White" => Some(Self::W),
            "U" | "Blue" => Some(Self::U),
            "B" | "Black" => Some(Self::B),
            "R" | "Red" => Some(Self::R),
            "G" | "Green" => Some(Self::G),
            _ => None,
        }
    }

    /// Deduplicates `colors` into WUBRG order.
    pub fn identity(colors: impl IntoIterator<Item = Color>) -> Vec<Color> {
        let mut present = [false; 5];
        for color in colors {
            if let Some(slot) = present.get_mut(color.index()) {
                *slot = true;
            }
        }
        Self::ALL
            .into_iter()
            .filter(|color| present.get(color.index()).copied().unwrap_or(false))
            .collect()
    }

    fn index(self) -> usize {
        match self {
            Self::W => 0,
            Self::U => 1,
            Self::B => 2,
            Self::R => 3,
            Self::G => 4,
        }
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

/// Scryfall rarities.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
pub enum Rarity {
    /// Common.
    Common,
    /// Uncommon.
    Uncommon,
    /// Rare.
    Rare,
    /// Mythic rare.
    Mythic,
    /// Special (timeshifted and similar).
    Special,
    /// Bonus sheet.
    Bonus,
}

impl Rarity {
    /// The stored text value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Common => "common",
            Self::Uncommon => "uncommon",
            Self::Rare => "rare",
            Self::Mythic => "mythic",
            Self::Special => "special",
            Self::Bonus => "bonus",
        }
    }
}

impl fmt::Display for Rarity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A card's legality in one format.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Legality {
    /// Legal.
    Legal,
    /// Not legal.
    NotLegal,
    /// Restricted to one copy.
    Restricted,
    /// Banned.
    Banned,
}

/// Where a card can be played, from Scryfall's `games`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Game {
    /// Paper Magic.
    Paper,
    /// Magic Arena.
    Arena,
    /// Magic Online.
    Mtgo,
    /// Any other platform Scryfall reports.
    #[serde(other)]
    Other,
}

/// Where a card sits in a deck.
///
/// Both apps collapse sideboards and maybeboards into `Considering`; the
/// Elixir code calls those cards ideas rather than deck contents.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
pub enum Zone {
    /// The main deck.
    Mainboard,
    /// The command zone.
    Commander,
    /// Sideboard, maybeboard, and other cards under consideration.
    Considering,
}

impl Zone {
    /// The stored text value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Mainboard => "mainboard",
            Self::Commander => "commander",
            Self::Considering => "considering",
        }
    }

    /// Parses a stored or remote zone name. Legacy `"sideboard"` and
    /// `"maybeboard"` values map to [`Zone::Considering`].
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "mainboard" => Some(Self::Mainboard),
            "commander" => Some(Self::Commander),
            "considering" | "sideboard" | "maybeboard" => Some(Self::Considering),
            _ => None,
        }
    }

    /// Whether cards in this zone are part of the playable deck.
    #[must_use]
    pub fn in_deck(self) -> bool {
        !matches!(self, Self::Considering)
    }
}

impl fmt::Display for Zone {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Scryfall layouts that mark a card as a token (or emblem) rather than a
/// playable card. Emblems count because they are printed on the backs of
/// tokens in the same token sets.
pub const TOKEN_LAYOUTS: [&str; 3] = ["token", "double_faced_token", "emblem"];

/// Whether a Scryfall layout is a token or emblem.
#[must_use]
pub fn is_token_layout(layout: &str) -> bool {
    TOKEN_LAYOUTS.contains(&layout)
}

/// Splits a type line into its supertypes/types and its subtypes.
///
/// `"Legendary Creature — Time Lord Doctor"` becomes
/// `("Legendary Creature", Some("Time Lord Doctor"))`.
#[must_use]
pub fn split_type_line(type_line: &str) -> (&str, Option<&str>) {
    match type_line.split_once('—') {
        Some((types, subtypes)) => (types.trim(), Some(subtypes.trim())),
        None => (type_line.trim(), None),
    }
}

/// Whether a type line carries the given supertype or card type, such as
/// `"Legendary"` or `"Creature"`, before the em dash.
#[must_use]
pub fn has_type(type_line: &str, word: &str) -> bool {
    split_type_line(type_line)
        .0
        .split_whitespace()
        .any(|part| part == word)
}

/// Whether a type line is a basic land, such as `"Basic Land — Plains"`,
/// `"Basic Snow Land — Forest"`, or `"Basic Land"` (Wastes). Snow basics
/// count; `"Snow Land — Forest Island"` does not.
#[must_use]
pub fn is_basic_land(type_line: &str) -> bool {
    has_type(type_line, "Basic") && has_type(type_line, "Land")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_land_type_lines() {
        assert!(is_basic_land("Basic Land — Plains"));
        assert!(is_basic_land("Basic Snow Land — Forest"));
        assert!(is_basic_land("Basic Land"));
        assert!(!is_basic_land("Snow Land — Forest Island"));
        assert!(!is_basic_land("Legendary Land"));
        assert!(!is_basic_land("Artifact — Basic"));
        assert!(!is_basic_land("Land — Basic"));
    }

    #[test]
    fn token_layouts() {
        assert!(is_token_layout("token"));
        assert!(is_token_layout("emblem"));
        assert!(!is_token_layout("normal"));
    }

    #[test]
    fn colors_dedupe_into_wubrg_order() {
        let colors = Color::identity([Color::G, Color::U, Color::G, Color::W]);
        assert_eq!(colors, vec![Color::W, Color::U, Color::G]);
        assert_eq!(Color::parse("Blue"), Some(Color::U));
        assert_eq!(Color::parse("C"), None);
    }

    #[test]
    fn preferred_finish_keeps_current_when_available() {
        let available = [Finish::Foil, Finish::Etched];
        assert_eq!(
            Finish::preferred(&available, Some(Finish::Etched)),
            Finish::Etched
        );
        assert_eq!(
            Finish::preferred(&available, Some(Finish::Nonfoil)),
            Finish::Foil
        );
        assert_eq!(Finish::preferred(&[], None), Finish::Nonfoil);
    }

    #[test]
    fn zone_parses_legacy_boards() {
        assert_eq!(Zone::parse("sideboard"), Some(Zone::Considering));
        assert_eq!(Zone::parse("maybeboard"), Some(Zone::Considering));
        assert_eq!(Zone::parse("commander"), Some(Zone::Commander));
        assert_eq!(Zone::parse("library"), None);
        assert_eq!(
            serde_json::to_string(&Zone::Considering).unwrap(),
            "\"considering\""
        );
    }

    #[test]
    fn enums_round_trip_through_serde() {
        let legality: Legality = serde_json::from_str("\"not_legal\"").unwrap();
        assert_eq!(legality, Legality::NotLegal);
        let game: Game = serde_json::from_str("\"sega\"").unwrap();
        assert_eq!(game, Game::Other);
        let rarity: Rarity = serde_json::from_str("\"mythic\"").unwrap();
        assert_eq!(rarity, Rarity::Mythic);
        assert!(serde_json::from_str::<Finish>("\"glossy\"").is_err());
    }
}
