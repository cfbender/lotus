//! Commander-format rules derived from a card's type line and Oracle text
//! (`TheGathering.Catalog.CardData`).

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::card::split_type_line;

/// How a card can share the command zone with another card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
pub enum CommanderPairing {
    /// The card is a Background, chosen by a commander with "Choose a Background".
    Background,
    /// "Friends forever" (or "Partner—Friends forever").
    FriendsForever,
    /// "Choose a Background".
    ChooseABackground,
    /// "Partner with `<name>`".
    PartnerWith,
    /// "Doctor's companion".
    DoctorsCompanion,
    /// Plain "Partner", including the "Partner—`<group>`" variants.
    Partner,
    /// A legendary Time Lord Doctor, pairable with a Doctor's companion.
    Doctor,
}

impl CommanderPairing {
    /// The stored text value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::FriendsForever => "friends_forever",
            Self::ChooseABackground => "choose_a_background",
            Self::PartnerWith => "partner_with",
            Self::DoctorsCompanion => "doctors_companion",
            Self::Partner => "partner",
            Self::Doctor => "doctor",
        }
    }
}

impl fmt::Display for CommanderPairing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

static BACKGROUND: LazyLock<Regex> = LazyLock::new(|| regex(r"(?i)(?:^|\s|—)Background(?:\s|$)"));
static EXPLICIT_COMMANDER: LazyLock<Regex> = LazyLock::new(|| regex(r"(?i)can be your commander"));
static TIME_LORD: LazyLock<Regex> = LazyLock::new(|| regex(r"\bTime Lord\b"));
static DOCTOR: LazyLock<Regex> = LazyLock::new(|| regex(r"\bDoctor\b"));

/// Oracle keyword lines, optionally followed by reminder text. Current Oracle
/// wording groups the pairing variants under Partner ("Partner—Friends
/// forever", "Partner—Survivors"); older wording printed "Friends forever"
/// alone. Order matters: the first match wins.
static PAIRING_RULES: LazyLock<[(CommanderPairing, Regex); 5]> = LazyLock::new(|| {
    [
        (
            CommanderPairing::FriendsForever,
            regex(r"(?i)(?:^|\n)(?:Partner—)?Friends forever\b"),
        ),
        (
            CommanderPairing::ChooseABackground,
            regex(r"(?i)(?:^|\n)Choose a Background\b"),
        ),
        (
            CommanderPairing::PartnerWith,
            regex(r"(?i)(?:^|\n)Partner with "),
        ),
        (
            CommanderPairing::DoctorsCompanion,
            regex(r"(?i)(?:^|\n)Doctor['’]s companion\b"),
        ),
        (
            CommanderPairing::Partner,
            regex(r"(?i)(?:^|\n)Partner(?:—|\s|$)"),
        ),
    ]
});

pub(crate) use crate::regex::compile as regex;

fn is_background(type_line: &str) -> bool {
    BACKGROUND.is_match(type_line)
}

fn is_legendary_creature(types: &str) -> bool {
    types.contains("Legendary") && types.contains("Creature")
}

/// Whether a card may lead a Commander deck: a legendary creature, or a card
/// whose text says it can be your commander. Backgrounds are never
/// commanders even though they are legendary.
#[must_use]
pub fn can_be_commander(type_line: &str, oracle_text: &str) -> bool {
    !is_background(type_line)
        && (is_legendary_creature(type_line) || EXPLICIT_COMMANDER.is_match(oracle_text))
}

/// A Doctor's companion pairs with a legendary creature that is both a Time
/// Lord and a Doctor.
fn is_doctor(type_line: &str) -> bool {
    match split_type_line(type_line) {
        (types, Some(subtypes)) => {
            is_legendary_creature(types)
                && TIME_LORD.is_match(subtypes)
                && DOCTOR.is_match(subtypes)
        }
        (_, None) => false,
    }
}

/// The pairing mechanic a card offers, if any.
#[must_use]
pub fn commander_pairing(type_line: &str, oracle_text: &str) -> Option<CommanderPairing> {
    if is_background(type_line) {
        return Some(CommanderPairing::Background);
    }
    if let Some((pairing, _)) = PAIRING_RULES
        .iter()
        .find(|(_, pattern)| pattern.is_match(oracle_text))
    {
        return Some(*pairing);
    }
    is_doctor(type_line).then_some(CommanderPairing::Doctor)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CREATURE: &str = "Legendary Creature — Human";

    #[test]
    fn every_pattern_compiles() {
        assert_eq!(PAIRING_RULES.len(), 5);
        assert!(BACKGROUND.is_match("Legendary Enchantment — Background"));
        assert!(EXPLICIT_COMMANDER.is_match("This can be your commander."));
    }

    #[test]
    fn derives_commander_eligibility_without_treating_backgrounds_as_commanders() {
        assert!(can_be_commander("Legendary Creature — Human Wizard", ""));
        assert!(!can_be_commander("Legendary Artifact", ""));
        assert!(can_be_commander(
            "Legendary Planeswalker — Test",
            "Test can be your commander."
        ));
        assert!(!can_be_commander("Legendary Enchantment — Background", ""));
    }

    #[test]
    fn represents_partner_mechanics_separately_from_eligibility() {
        assert_eq!(
            commander_pairing(CREATURE, "Partner"),
            Some(CommanderPairing::Partner)
        );
        assert_eq!(
            commander_pairing(CREATURE, "Friends forever"),
            Some(CommanderPairing::FriendsForever)
        );
        assert_eq!(
            commander_pairing(CREATURE, "Choose a Background"),
            Some(CommanderPairing::ChooseABackground)
        );
        assert_eq!(
            commander_pairing("Legendary Enchantment — Background", ""),
            Some(CommanderPairing::Background)
        );
    }

    #[test]
    fn recognizes_current_oracle_wording_with_reminder_text() {
        assert_eq!(
            commander_pairing(
                CREATURE,
                "Reach\nChoose a Background (You can have a Background as a second commander.)"
            ),
            Some(CommanderPairing::ChooseABackground)
        );
        assert_eq!(
            commander_pairing(
                CREATURE,
                "Goad that creature.\nPartner—Friends forever (You can have two commanders if both have this ability.)"
            ),
            Some(CommanderPairing::FriendsForever)
        );
        assert_eq!(
            commander_pairing(
                CREATURE,
                "Menace\nPartner—Survivors (You can have two commanders if both have this ability.)"
            ),
            Some(CommanderPairing::Partner)
        );
        assert_eq!(
            commander_pairing(
                CREATURE,
                "Draw a card.\nPartner (You can have two commanders if both have partner.)"
            ),
            Some(CommanderPairing::Partner)
        );
        assert_eq!(
            commander_pairing(
                CREATURE,
                "Partner with Toothy, Imaginary Friend (When this creature enters, …)"
            ),
            Some(CommanderPairing::PartnerWith)
        );
        assert_eq!(
            commander_pairing(
                "Legendary Creature — Human Advisor",
                "Doctor's companion (You can have two commanders if the other is the Doctor.)"
            ),
            Some(CommanderPairing::DoctorsCompanion)
        );
    }

    #[test]
    fn marks_time_lord_doctors_as_pairable() {
        assert_eq!(
            commander_pairing("Legendary Creature — Time Lord Doctor", "Haste"),
            Some(CommanderPairing::Doctor)
        );
        assert_eq!(
            commander_pairing("Legendary Creature — Time Lord Scientist", ""),
            None
        );
        assert_eq!(commander_pairing("Creature — Time Lord Doctor", ""), None);
        assert_eq!(
            commander_pairing("Legendary Creature — Goblin Warrior", "Flying"),
            None
        );
    }

    #[test]
    fn ignores_pairing_words_that_are_not_keyword_lines() {
        assert_eq!(
            commander_pairing(
                CREATURE,
                "Whenever you cast a Doctor spell or creature spell with doctor's companion, draw a card."
            ),
            None
        );
        assert_eq!(commander_pairing(CREATURE, "Partners in crime"), None);
    }
}
