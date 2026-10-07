//! The Scryfall card object, as served by the API and the bulk-data files.
//!
//! Only the fields the apps use are modeled. Unknown fields are ignored on
//! decode, so a bulk file with new Scryfall fields still imports. Absent
//! optional fields decode to `None` or an empty collection.
//!
//! Fixed-vocabulary fields (`rarity`, `finishes`, `colors`, `legalities`,
//! `all_parts`) are decoded leniently: a value this crate does not know is
//! dropped instead of failing the whole card. The Elixir apps stored those
//! values as raw strings and never validated them; a Rust port can only
//! store what its enums represent, so the choice is between skipping one
//! value and skipping the card, and one new Scryfall vocabulary word must
//! not make a bulk import lose a card.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};
use time::Date;
use time::macros::format_description;

use crate::card::{Color, Finish, Game, Legality, Rarity, is_token_layout};
use crate::ids::{OracleId, ScryfallId};

/// Image URIs for a card or face.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageUris {
    /// Small JPG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub small: Option<String>,
    /// Normal JPG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub normal: Option<String>,
    /// Large JPG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub large: Option<String>,
    /// Full-resolution PNG.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub png: Option<String>,
    /// Art crop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_crop: Option<String>,
    /// Border crop.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub border_crop: Option<String>,
}

/// One face of a multi-faced card.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CardFace {
    /// The face's oracle id (present on reversible cards).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_id: Option<OracleId>,
    /// The face's name.
    #[serde(default)]
    pub name: String,
    /// The face's type line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_line: Option<String>,
    /// The face's mana cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    /// The face's mana value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmc: Option<f64>,
    /// The face's Oracle text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_text: Option<String>,
    /// The face's colors.
    #[serde(
        default,
        deserialize_with = "lenient_opt_vec",
        skip_serializing_if = "Option::is_none"
    )]
    pub colors: Option<Vec<Color>>,
    /// The face's illustration id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub illustration_id: Option<String>,
    /// The face's flavor name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor_name: Option<String>,
    /// The face's flavor text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor_text: Option<String>,
    /// The face's images.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_uris: Option<ImageUris>,
    /// Power.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub power: Option<String>,
    /// Toughness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toughness: Option<String>,
    /// Loyalty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loyalty: Option<String>,
}

/// A card related to this one, from Scryfall's `all_parts`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RelatedCard {
    /// The related printing.
    pub id: ScryfallId,
    /// The relationship: `token`, `meld_part`, `meld_result`, or `combo_piece`.
    #[serde(default)]
    pub component: String,
    /// The related card's name.
    #[serde(default)]
    pub name: String,
}

/// A Scryfall card object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScryfallCard {
    /// The printing id.
    pub id: ScryfallId,
    /// The oracle id. Reversible cards carry it on each face instead; see
    /// [`ScryfallCard::with_face_identity`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_id: Option<OracleId>,
    /// The card name. Multi-faced cards use `"Front // Back"`.
    pub name: String,
    /// Language code, such as `"en"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    /// Scryfall layout, such as `"normal"` or `"transform"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    /// The type line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_line: Option<String>,
    /// Oracle text for single-faced cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle_text: Option<String>,
    /// Mana cost for single-faced cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mana_cost: Option<String>,
    /// Mana value.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cmc: Option<f64>,
    /// Colors for single-faced cards.
    #[serde(
        default,
        deserialize_with = "lenient_opt_vec",
        skip_serializing_if = "Option::is_none"
    )]
    pub colors: Option<Vec<Color>>,
    /// Color identity.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub color_identity: Vec<Color>,
    /// Format legalities. Formats whose value is not a known [`Legality`]
    /// are dropped.
    #[serde(default, deserialize_with = "lenient_map")]
    pub legalities: BTreeMap<String, Legality>,
    /// Whether the card is on the Commander Game Changers list.
    #[serde(default)]
    pub game_changer: bool,
    /// EDHREC popularity rank.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edhrec_rank: Option<u32>,
    /// URI of the card's rulings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rulings_uri: Option<String>,
    /// Set code, as Scryfall sends it (lowercase).
    #[serde(default)]
    pub set: String,
    /// Set name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_name: Option<String>,
    /// Set type, such as `"expansion"`, `"token"`, or `"memorabilia"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_type: Option<String>,
    /// Collector number.
    #[serde(default)]
    pub collector_number: String,
    /// Illustration id for single-faced cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub illustration_id: Option<String>,
    /// Flavor name for single-faced cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor_name: Option<String>,
    /// Flavor text for single-faced cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flavor_text: Option<String>,
    /// Rarity. An unknown rarity decodes as `None`.
    #[serde(
        default,
        deserialize_with = "lenient_opt",
        skip_serializing_if = "Option::is_none"
    )]
    pub rarity: Option<Rarity>,
    /// Available finishes, without any this crate does not know.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub finishes: Vec<Finish>,
    /// Promo types.
    #[serde(default)]
    pub promo_types: Vec<String>,
    /// Whether this is a promo printing.
    #[serde(default)]
    pub promo: bool,
    /// Whether this is a digital-only printing.
    #[serde(default)]
    pub digital: bool,
    /// Platforms the printing exists on.
    #[serde(default)]
    pub games: Vec<Game>,
    /// Images for single-faced cards.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_uris: Option<ImageUris>,
    /// Prices by currency key (`usd`, `usd_foil`, ...), as decimal strings.
    #[serde(default)]
    pub prices: BTreeMap<String, Option<String>>,
    /// Release date. An unparseable date decodes as `None`, as it does in
    /// both apps.
    #[serde(
        default,
        deserialize_with = "lenient_date",
        skip_serializing_if = "Option::is_none"
    )]
    pub released_at: Option<Date>,
    /// `TCGplayer` product id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcgplayer_id: Option<u64>,
    /// `TCGplayer` product id for the etched printing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tcgplayer_etched_id: Option<u64>,
    /// Related cards (tokens, meld parts, combo pieces). Parts without an
    /// `id` are dropped, as ManaVault's `card_token_rows/1` skipped them.
    #[serde(default, deserialize_with = "lenient_vec")]
    pub all_parts: Vec<RelatedCard>,
    /// Faces of a multi-faced card.
    #[serde(default)]
    pub card_faces: Vec<CardFace>,
}

/// Decodes `T` from an already-parsed JSON value, or `None` when it is not
/// one this crate represents.
fn lenient<T: DeserializeOwned>(value: serde_json::Value) -> Option<T> {
    serde_json::from_value(value).ok()
}

fn lenient_opt<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    let raw: Option<serde_json::Value> = Option::deserialize(deserializer)?;
    Ok(raw.and_then(lenient))
}

fn lenient_vec<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<Vec<T>, D::Error> {
    let raw: Option<Vec<serde_json::Value>> = Option::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(lenient)
        .collect())
}

fn lenient_opt_vec<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<Option<Vec<T>>, D::Error> {
    let raw: Option<Vec<serde_json::Value>> = Option::deserialize(deserializer)?;
    Ok(raw.map(|values| values.into_iter().filter_map(lenient).collect()))
}

fn lenient_map<'de, D: Deserializer<'de>, T: DeserializeOwned>(
    deserializer: D,
) -> Result<BTreeMap<String, T>, D::Error> {
    let raw: Option<BTreeMap<String, serde_json::Value>> = Option::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| lenient(value).map(|value| (key, value)))
        .collect())
}

fn lenient_date<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Date>, D::Error> {
    let raw: Option<String> = Option::deserialize(deserializer)?;
    Ok(raw.as_deref().and_then(parse_date))
}

/// Parses a `YYYY-MM-DD` date, returning `None` for anything else.
#[must_use]
pub fn parse_date(value: &str) -> Option<Date> {
    Date::parse(value, format_description!("[year]-[month]-[day]")).ok()
}

const FACE_SEPARATOR: &str = "\n---\n";

fn join_faces<'a>(parts: impl Iterator<Item = Option<&'a str>>) -> Option<String> {
    let present: Vec<&str> = parts.flatten().collect();
    (!present.is_empty()).then(|| present.join(FACE_SEPARATOR))
}

impl ScryfallCard {
    /// The first face, when the card has faces.
    #[must_use]
    pub fn front_face(&self) -> Option<&CardFace> {
        self.card_faces.first()
    }

    /// Whether the printing exists in paper.
    #[must_use]
    pub fn is_paper(&self) -> bool {
        self.games.contains(&Game::Paper)
    }

    /// Whether the card is a token or emblem.
    #[must_use]
    pub fn is_token(&self) -> bool {
        self.layout.as_deref().is_some_and(is_token_layout)
    }

    /// Whether the card is legal in Commander.
    #[must_use]
    pub fn commander_legal(&self) -> bool {
        self.legalities.get("commander") == Some(&Legality::Legal)
    }

    /// Reversible cards (e.g. "Temple Garden // Temple Garden") have no
    /// top-level oracle id; it lives on each face. This copies the identity
    /// fields (oracle id, name, type line, mana cost, mana value, Oracle
    /// text) from the first face so the card matches the canonical card
    /// sharing that oracle id. Cards that already have an oracle id are
    /// returned unchanged.
    #[must_use]
    pub fn with_face_identity(mut self) -> Self {
        if self.oracle_id.is_some() {
            return self;
        }
        let Some(face) = self.card_faces.first() else {
            return self;
        };
        let Some(oracle_id) = face.oracle_id.clone() else {
            return self;
        };
        self.oracle_id = Some(oracle_id);
        self.name.clone_from(&face.name);
        if let Some(type_line) = &face.type_line {
            self.type_line = Some(type_line.clone());
        }
        if let Some(mana_cost) = &face.mana_cost {
            self.mana_cost = Some(mana_cost.clone());
        }
        if let Some(cmc) = face.cmc {
            self.cmc = Some(cmc);
        }
        if let Some(oracle_text) = &face.oracle_text {
            self.oracle_text = Some(oracle_text.clone());
        }
        self
    }

    /// Oracle text: the card's own, or every face's joined with `\n---\n`.
    #[must_use]
    pub fn full_oracle_text(&self) -> Option<String> {
        match &self.oracle_text {
            Some(text) => Some(text.clone()),
            None => join_faces(
                self.card_faces
                    .iter()
                    .map(|face| face.oracle_text.as_deref()),
            ),
        }
    }

    /// Flavor text: the card's own, or every face's joined with `\n---\n`.
    #[must_use]
    pub fn full_flavor_text(&self) -> Option<String> {
        match &self.flavor_text {
            Some(text) => Some(text.clone()),
            None => join_faces(
                self.card_faces
                    .iter()
                    .map(|face| face.flavor_text.as_deref()),
            ),
        }
    }

    /// Flavor name: the card's own, or every face's joined with `\n---\n`.
    #[must_use]
    pub fn full_flavor_name(&self) -> Option<String> {
        match &self.flavor_name {
            Some(name) => Some(name.clone()),
            None => join_faces(
                self.card_faces
                    .iter()
                    .map(|face| face.flavor_name.as_deref()),
            ),
        }
    }

    /// Mana cost: the card's own, or the front face's.
    #[must_use]
    pub fn front_mana_cost(&self) -> Option<&str> {
        self.mana_cost
            .as_deref()
            .or_else(|| self.front_face().and_then(|face| face.mana_cost.as_deref()))
    }

    /// Colors: the card's own, or the front face's, or none.
    #[must_use]
    pub fn front_colors(&self) -> &[Color] {
        self.colors
            .as_deref()
            .or_else(|| self.front_face().and_then(|face| face.colors.as_deref()))
            .unwrap_or_default()
    }

    /// Illustration id: the card's own, or the first face that has one.
    #[must_use]
    pub fn any_illustration_id(&self) -> Option<&str> {
        self.illustration_id.as_deref().or_else(|| {
            self.card_faces
                .iter()
                .find_map(|face| face.illustration_id.as_deref())
        })
    }

    /// Images: the card's own, or the front face's.
    #[must_use]
    pub fn front_image_uris(&self) -> Option<&ImageUris> {
        self.image_uris
            .as_ref()
            .or_else(|| self.front_face().and_then(|face| face.image_uris.as_ref()))
    }

    /// Images of every face that has them (the card's own first), for
    /// storing all faces' images together.
    #[must_use]
    pub fn all_image_uris(&self) -> Vec<&ImageUris> {
        match &self.image_uris {
            Some(uris) => vec![uris],
            None => self
                .card_faces
                .iter()
                .filter_map(|face| face.image_uris.as_ref())
                .collect(),
        }
    }

    /// The tokens this printing creates, from `all_parts`, deduplicated and
    /// in order. Only producer → token links are kept: a token's own
    /// `all_parts` lists every producer ever printed, so tokens yield none.
    #[must_use]
    pub fn token_ids(&self) -> Vec<&ScryfallId> {
        if self.is_token() {
            return Vec::new();
        }
        let mut seen = Vec::new();
        for part in &self.all_parts {
            if part.component == "token" && !seen.contains(&&part.id) {
                seen.push(&part.id);
            }
        }
        seen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::date;

    fn card(json: &str) -> ScryfallCard {
        serde_json::from_str(json).unwrap()
    }

    /// A vocabulary word this crate does not know drops that value, not the
    /// card; known values beside it survive.
    #[test]
    fn drops_unknown_vocabulary_instead_of_failing_the_card() {
        let card = card(
            r#"{"id":"p1","name":"X","rarity":"glossy","finishes":["nonfoil","glossy","etched"],
                "colors":["W","Purple"],"color_identity":["Purple","G"],
                "legalities":{"commander":"legal","future":"pending","vintage":"restricted"},
                "all_parts":[{"component":"token","name":"No id"},{"id":"t1","component":"token","name":"Soldier"}],
                "card_faces":[{"name":"Front","colors":["U","Purple"]}]}"#,
        );
        assert_eq!(card.rarity, None);
        assert_eq!(card.finishes, vec![Finish::Nonfoil, Finish::Etched]);
        assert_eq!(card.colors, Some(vec![Color::W]));
        assert_eq!(card.color_identity, vec![Color::G]);
        assert_eq!(
            card.legalities,
            BTreeMap::from([
                ("commander".to_owned(), Legality::Legal),
                ("vintage".to_owned(), Legality::Restricted),
            ])
        );
        assert_eq!(card.all_parts.len(), 1);
        assert_eq!(card.all_parts[0].id.as_str(), "t1");
        assert_eq!(card.card_faces[0].colors, Some(vec![Color::U]));

        let known = super::tests::card(r#"{"id":"p2","name":"Y","rarity":"bonus","colors":null}"#);
        assert_eq!(known.rarity, Some(Rarity::Bonus));
        assert_eq!(known.colors, None);
    }

    #[test]
    fn decodes_minimal_record_with_defaults() {
        let card = card(r#"{"id":"p1","name":"Rhystic Study","oracle_id":"o1"}"#);
        assert_eq!(card.name, "Rhystic Study");
        assert!(!card.game_changer);
        assert!(card.finishes.is_empty());
        assert!(!card.is_paper());
        assert_eq!(card.released_at, None);
        assert!(serde_json::from_str::<ScryfallCard>(r#"{"name":"no id"}"#).is_err());
    }

    #[test]
    fn lenient_dates_and_unknown_fields() {
        let card = card(
            r#"{"id":"p","name":"n","released_at":"2024-02-29","unknown_field":1,"games":["paper","sega"]}"#,
        );
        assert_eq!(card.released_at, Some(date!(2024 - 02 - 29)));
        assert!(card.is_paper());
        let bad = card_with_date("not-a-date");
        assert_eq!(bad.released_at, None);
        let odd = card_with_date("2024-13-01");
        assert_eq!(odd.released_at, None);
    }

    fn card_with_date(value: &str) -> ScryfallCard {
        card(&format!(
            r#"{{"id":"p","name":"n","released_at":"{value}"}}"#
        ))
    }

    #[test]
    fn reversible_cards_take_identity_from_first_face() {
        let card = card(
            r#"{"id":"p","name":"Temple Garden // Temple Garden","layout":"reversible_card",
                "card_faces":[{"oracle_id":"o-tg","name":"Temple Garden","type_line":"Land — Forest Plains","mana_cost":"","cmc":0,"oracle_text":"..."},
                              {"oracle_id":"o-tg","name":"Temple Garden"}]}"#,
        )
        .with_face_identity();
        assert_eq!(card.oracle_id.as_ref().map(OracleId::as_str), Some("o-tg"));
        assert_eq!(card.name, "Temple Garden");
        assert_eq!(card.type_line.as_deref(), Some("Land — Forest Plains"));
        assert_eq!(card.cmc, Some(0.0));
    }

    #[test]
    fn cards_with_an_oracle_id_keep_their_own_identity() {
        let card = card(
            r#"{"id":"p","oracle_id":"o","name":"Delver of Secrets // Insectile Aberration",
                "card_faces":[{"oracle_id":"other","name":"Delver of Secrets","oracle_text":"front"},{"name":"Insectile Aberration","oracle_text":"back"}]}"#,
        )
        .with_face_identity();
        assert_eq!(card.name, "Delver of Secrets // Insectile Aberration");
        assert_eq!(card.full_oracle_text().as_deref(), Some("front\n---\nback"));
    }

    #[test]
    fn face_fallbacks() {
        let card = card(
            r#"{"id":"p","name":"n","card_faces":[
                {"name":"a","colors":["R"],"mana_cost":"{R}","image_uris":{"normal":"a.jpg"},"flavor_text":"fa"},
                {"name":"b","illustration_id":"ill-b","image_uris":{"normal":"b.jpg"}}]}"#,
        );
        assert_eq!(card.front_colors(), &[Color::R]);
        assert_eq!(card.front_mana_cost(), Some("{R}"));
        assert_eq!(card.any_illustration_id(), Some("ill-b"));
        assert_eq!(
            card.front_image_uris().and_then(|u| u.normal.as_deref()),
            Some("a.jpg")
        );
        assert_eq!(card.all_image_uris().len(), 2);
        assert_eq!(card.full_flavor_text().as_deref(), Some("fa"));
        assert_eq!(card.full_flavor_name(), None);
    }

    #[test]
    fn token_links_only_from_producers() {
        let producer = card(
            r#"{"id":"p","name":"n","all_parts":[
                {"id":"t1","component":"token","name":"Bird"},
                {"id":"t1","component":"token","name":"Bird"},
                {"id":"m","component":"meld_part","name":"x"},
                {"id":"t2","component":"token","name":"Cat"}]}"#,
        );
        assert_eq!(
            producer
                .token_ids()
                .iter()
                .map(|id| id.as_str())
                .collect::<Vec<_>>(),
            vec!["t1", "t2"]
        );
        let token = card(
            r#"{"id":"t","name":"Bird","layout":"token","all_parts":[{"id":"p","component":"token","name":"n"}]}"#,
        );
        assert!(token.token_ids().is_empty());
        assert!(token.is_token());
    }

    #[test]
    fn commander_legality() {
        let legal =
            card(r#"{"id":"p","name":"n","legalities":{"commander":"legal","modern":"banned"}}"#);
        assert!(legal.commander_legal());
        let banned = card(r#"{"id":"p","name":"n","legalities":{"commander":"banned"}}"#);
        assert!(!banned.commander_legal());
    }
}
