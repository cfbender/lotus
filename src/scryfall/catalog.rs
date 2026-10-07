//! Which Scryfall records belong in a card catalog, and which printing
//! represents a card.
//!
//! The two apps filter the bulk file differently: ManaVault keeps paper
//! printings plus tokens ([`import_policy`]); the-gathering keeps one
//! representative printing per oracle id ([`describes_card`] and
//! [`SelectionKey`]). Both policies live here so neither app re-derives
//! them.

use std::cmp::Ordering;

use crate::scryfall::ScryfallCard;

/// Set types whose non-token cards are never real cards (ManaVault's
/// `@excluded_set_types`).
pub const EXCLUDED_SET_TYPES: [&str; 2] = ["memorabilia", "token"];

/// Set types that print rules inserts with the bare type line `"Card"`
/// (ManaVault's `@insert_set_types`).
pub const INSERT_SET_TYPES: [&str; 2] = ["memorabilia", "minigame"];

/// Whether any face of the type line is exactly `"Card"`, which Scryfall
/// uses for helper tokens (The Monarch, On an Adventure), checklists, and
/// substitute cards.
#[must_use]
pub fn is_bare_card(type_line: Option<&str>) -> bool {
    type_line.is_some_and(|line| line.split("//").any(|face| face.trim() == "Card"))
}

/// Whether a record is a non-game insert: a bare `"Card"` from a memorabilia
/// or minigame set, or a checklist / substitute card.
#[must_use]
pub fn is_non_game_insert(card: &ScryfallCard) -> bool {
    is_bare_card(card.type_line.as_deref())
        && (card
            .set_type
            .as_deref()
            .is_some_and(|set_type| INSERT_SET_TYPES.contains(&set_type))
            || card.name.contains("Checklist")
            || card.name.contains("Substitute Card"))
}

/// Why ManaVault's import leaves a record out, if it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exclusion {
    /// Not printed in paper.
    NotPaper,
    /// A checklist, substitute, or rules insert.
    NonGameInsert,
    /// A non-token record with the bare type line `"Card"`.
    BareCard,
    /// A non-token record from a memorabilia or token set.
    ExcludedSetType,
}

/// ManaVault's import policy (`Scryfall.Sync.paper_card?` and
/// `Scryfall.Import.excluded?`): keep paper printings, keep tokens and
/// emblems from token sets, and drop inserts and memorabilia. Returns the
/// first reason a record is excluded, or `None` to keep it.
#[must_use]
pub fn import_policy(card: &ScryfallCard) -> Option<Exclusion> {
    if !card.is_paper() {
        return Some(Exclusion::NotPaper);
    }
    if is_non_game_insert(card) {
        return Some(Exclusion::NonGameInsert);
    }
    if card.is_token() {
        return None;
    }
    if is_bare_card(card.type_line.as_deref()) {
        return Some(Exclusion::BareCard);
    }
    if card
        .set_type
        .as_deref()
        .is_some_and(|set_type| EXCLUDED_SET_TYPES.contains(&set_type))
    {
        return Some(Exclusion::ExcludedSetType);
    }
    None
}

/// the-gathering's catalog policy (`CardData.from_scryfall/1`): a record
/// describes a card unless it comes from a token or memorabilia set or lacks
/// an oracle id.
#[must_use]
pub fn describes_card(card: &ScryfallCard) -> bool {
    card.oracle_id.is_some()
        && !card
            .set_type
            .as_deref()
            .is_some_and(|set_type| EXCLUDED_SET_TYPES.contains(&set_type))
}

/// Ranks printings of one card so the greatest key is the printing to show:
/// English, paper, non-digital, non-promo, then newest release date, set,
/// collector number, and id. Compares the same way as the-gathering's
/// `selection_key/1` string, which `SelectionKey`'s `Display` impl reproduces
/// for storage alongside existing rows.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // four independent yes/no facts, compared in this order
pub struct SelectionKey {
    english: bool,
    paper: bool,
    physical: bool,
    not_promo: bool,
    released_at: String,
    set: String,
    collector_number: String,
    id: String,
}

impl SelectionKey {
    /// The key for a printing.
    #[must_use]
    pub fn of(card: &ScryfallCard) -> Self {
        Self {
            english: card.lang.as_deref() == Some("en"),
            paper: card.is_paper(),
            physical: !card.digital,
            not_promo: !card.promo,
            released_at: card
                .released_at
                .map_or_else(|| "0000-00-00".to_owned(), |date| date.to_string()),
            set: card.set.clone(),
            collector_number: card.collector_number.clone(),
            id: card.id.as_str().to_owned(),
        }
    }

    /// Parses a stored key string.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        let mut parts = value.splitn(8, '|');
        let mut bit = || match parts.next()? {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        };
        let english = bit()?;
        let paper = bit()?;
        let physical = bit()?;
        let not_promo = bit()?;
        let released_at = parts.next()?.to_owned();
        let set = parts.next()?.to_owned();
        let collector_number = parts.next()?.to_owned();
        let id = parts.next()?.to_owned();
        Some(Self {
            english,
            paper,
            physical,
            not_promo,
            released_at,
            set,
            collector_number,
            id,
        })
    }
}

impl std::fmt::Display for SelectionKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bit = |value: bool| if value { "1" } else { "0" };
        write!(
            f,
            "{}|{}|{}|{}|{}|{}|{}|{}",
            bit(self.english),
            bit(self.paper),
            bit(self.physical),
            bit(self.not_promo),
            self.released_at,
            self.set,
            self.collector_number,
            self.id
        )
    }
}

impl PartialOrd for SelectionKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SelectionKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.to_string().cmp(&other.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(json: &str) -> ScryfallCard {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn bare_card_type_lines() {
        assert!(is_bare_card(Some("Card")));
        assert!(is_bare_card(Some("Card // Card")));
        assert!(is_bare_card(Some(" Card ")));
        assert!(!is_bare_card(Some("Artifact Card")));
        assert!(!is_bare_card(None));
    }

    #[test]
    fn manavault_import_policy() {
        let paper = r#""games":["paper"]"#;
        let keep = card(&format!(
            r#"{{"id":"a","name":"Sol Ring","type_line":"Artifact",{paper}}}"#
        ));
        assert_eq!(import_policy(&keep), None);

        let digital = card(r#"{"id":"a","name":"Sol Ring","games":["arena"]}"#);
        assert_eq!(import_policy(&digital), Some(Exclusion::NotPaper));

        let monarch = card(&format!(
            r#"{{"id":"a","name":"The Monarch","type_line":"Card","layout":"token","set_type":"token",{paper}}}"#
        ));
        assert_eq!(import_policy(&monarch), None, "helper tokens stay");

        let checklist = card(&format!(
            r#"{{"id":"a","name":"Checklist Card","type_line":"Card","layout":"token","set_type":"token",{paper}}}"#
        ));
        assert_eq!(import_policy(&checklist), Some(Exclusion::NonGameInsert));

        let insert = card(&format!(
            r#"{{"id":"a","name":"Rules Tip","type_line":"Card","layout":"normal","set_type":"memorabilia",{paper}}}"#
        ));
        assert_eq!(import_policy(&insert), Some(Exclusion::NonGameInsert));

        let bare = card(&format!(
            r#"{{"id":"a","name":"Mystery","type_line":"Card","layout":"normal","set_type":"expansion",{paper}}}"#
        ));
        assert_eq!(import_policy(&bare), Some(Exclusion::BareCard));

        let memorabilia = card(&format!(
            r#"{{"id":"a","name":"Oversized","type_line":"Creature","layout":"normal","set_type":"memorabilia",{paper}}}"#
        ));
        assert_eq!(
            import_policy(&memorabilia),
            Some(Exclusion::ExcludedSetType)
        );

        let emblem = card(&format!(
            r#"{{"id":"a","name":"Emblem","type_line":"Emblem","layout":"emblem","set_type":"token",{paper}}}"#
        ));
        assert_eq!(import_policy(&emblem), None);
    }

    #[test]
    fn the_gathering_card_policy() {
        assert!(describes_card(&card(
            r#"{"id":"a","oracle_id":"o","name":"n"}"#
        )));
        assert!(!describes_card(&card(
            r#"{"id":"a","oracle_id":"o","name":"n","set_type":"token"}"#
        )));
        assert!(!describes_card(&card(
            r#"{"id":"a","oracle_id":"o","name":"n","set_type":"memorabilia"}"#
        )));
        assert!(!describes_card(&card(
            r#"{"id":"a","name":"no oracle id"}"#
        )));
    }

    #[test]
    fn selection_key_prefers_english_paper_physical_non_promo_newest() {
        let latest = card(
            r#"{"id":"printing-latest","name":"n","lang":"en","games":["paper"],"released_at":"2024-01-01","set":"new","collector_number":"1"}"#,
        );
        let older = card(
            r#"{"id":"printing-older","name":"n","lang":"en","games":["paper"],"released_at":"2020-01-01","set":"old","collector_number":"1"}"#,
        );
        let promo = card(
            r#"{"id":"printing-promo","name":"n","lang":"en","games":["paper"],"promo":true,"released_at":"2025-01-01","set":"pro","collector_number":"1"}"#,
        );
        let digital = card(
            r#"{"id":"printing-digital","name":"n","lang":"en","games":["arena"],"digital":true,"released_at":"2025-06-01","set":"dig","collector_number":"1"}"#,
        );
        let missing_date =
            card(r#"{"id":"printing-undated","name":"n","lang":"en","games":["paper"]}"#);

        assert_eq!(
            SelectionKey::of(&latest).to_string(),
            "1|1|1|1|2024-01-01|new|1|printing-latest"
        );
        assert_eq!(
            SelectionKey::of(&missing_date).to_string(),
            "1|1|1|1|0000-00-00|||printing-undated"
        );
        let mut keys = [&promo, &digital, &older, &latest, &missing_date]
            .map(SelectionKey::of)
            .to_vec();
        keys.sort();
        assert_eq!(keys.last().unwrap().id, "printing-latest");
        assert!(SelectionKey::of(&older) > SelectionKey::of(&promo));
        assert_eq!(
            SelectionKey::parse("1|1|1|1|2024-01-01|new|1|printing-latest"),
            Some(SelectionKey::of(&latest))
        );
        assert_eq!(SelectionKey::parse("1|x|1|1|d|s|c|i"), None);
        assert_eq!(SelectionKey::parse("1|1|1"), None);
    }
}
