//! Commander-format rules derived from a card's type line and Oracle text
//! (`Manavault.Catalog.CommanderRules` and `TheGathering.Catalog.CardData`).

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
static COMMANDER_TYPE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"\b(?:Creature|Vehicle|Spacecraft)\b"));
/// The Partner keyword on its own line, with the restricted label if any
/// ("Partner—Survivors"). "Partner with <name>" is a different mechanic and
/// deliberately does not match.
static PARTNER_LABEL: LazyLock<Regex> =
    LazyLock::new(|| regex(r"^Partner(?:\s*[—–-]\s*([^(]+?))?\s*(?:\(|$)"));
static FRIENDS_FOREVER_LINE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)^(?:Partner—)?Friends forever(?:$|\s*\()"));
static DOCTORS_COMPANION_LINE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)^Doctor['’]s companion(?:$|\s*\()"));
static CHOOSE_A_BACKGROUND_LINE: LazyLock<Regex> =
    LazyLock::new(|| regex(r"(?i)^Choose a Background(?:$|\s*\()"));

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

/// The front face of a type line: the part before `//`.
fn front_face(type_line: &str) -> &str {
    type_line.split("//").next().unwrap_or(type_line)
}

/// Whether a card may lead a Commander deck.
///
/// Per Comprehensive Rules 903.3 that is a legendary creature, Vehicle, or
/// Spacecraft card, judged by the front face of a multi-faced card, plus any
/// card whose text grants "can be your commander" (903.3a), such as
/// planeswalker commanders. Scryfall exposes no field for this, so the
/// Oracle text is the source of truth.
///
/// This is ManaVault's rule. the-gathering accepted only legendary creatures
/// and judged the whole type line, so it rejected legendary Vehicles and
/// accepted a card whose back face is a legendary creature; both are wrong
/// under 903.3, so the ManaVault rule is used for both apps.
#[must_use]
pub fn can_be_commander(type_line: &str, oracle_text: &str) -> bool {
    let front = front_face(type_line);
    (front.contains("Legendary") && COMMANDER_TYPE.is_match(front))
        || EXPLICIT_COMMANDER.is_match(oracle_text)
}

/// What the two-commander check needs from a card.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CommanderCard<'a> {
    /// The card name; multi-faced cards use `"Front // Back"`.
    pub name: &'a str,
    /// The type line.
    pub type_line: &'a str,
    /// The Oracle text, faces joined with newlines.
    pub oracle_text: &'a str,
}

impl CommanderCard<'_> {
    fn lines(&self) -> impl Iterator<Item = &str> {
        self.oracle_text.lines().map(str::trim)
    }

    /// The Partner keyword's restricted label, lowercased; an empty string
    /// for plain Partner; `None` without the keyword.
    fn partner_label(&self) -> Option<String> {
        self.lines().find_map(|line| {
            let captures = PARTNER_LABEL.captures(line)?;
            Some(
                captures
                    .get(1)
                    .map(|label| label.as_str().trim().to_lowercase())
                    .unwrap_or_default(),
            )
        })
    }

    fn partner_with(&self, other: &Self) -> bool {
        let other_name = regex::escape(other.base_name());
        let Ok(pattern) = Regex::new(&format!(r"(?i)^Partner with {other_name}(?:$|\s*\()")) else {
            return false;
        };
        self.lines().any(|line| pattern.is_match(line))
    }

    fn base_name(&self) -> &str {
        self.name.split(" // ").next().unwrap_or(self.name).trim()
    }

    fn has_line(&self, pattern: &Regex) -> bool {
        self.lines().any(|line| pattern.is_match(line))
    }
}

/// Whether two cards form a legal two-commander command zone
/// (`Manavault.Catalog.CommanderRules.valid_pair?/2`): both have the Partner
/// keyword with the same restricted label (plain Partner pairs only with
/// plain Partner, "Partner—Survivors" only with another "Partner—Survivors"),
/// each names the other with "Partner with", both have Friends forever, one
/// is a Doctor's companion and the other a legendary Time Lord Doctor, or one
/// chooses a Background and the other is a Background.
///
/// ManaVault accepted any card with "Time Lord Doctor" in its type line as
/// the Doctor; a non-legendary one cannot be a commander, so this requires a
/// legendary creature as [`commander_pairing`] does.
#[must_use]
pub fn valid_pair(a: &CommanderCard<'_>, b: &CommanderCard<'_>) -> bool {
    let partner_keyword = match (a.partner_label(), b.partner_label()) {
        (Some(label_a), Some(label_b)) => label_a == label_b,
        _ => false,
    };
    partner_keyword
        || (a.partner_with(b) && b.partner_with(a))
        || (a.has_line(&FRIENDS_FOREVER_LINE) && b.has_line(&FRIENDS_FOREVER_LINE))
        || (a.has_line(&DOCTORS_COMPANION_LINE) && is_doctor(b.type_line))
        || (b.has_line(&DOCTORS_COMPANION_LINE) && is_doctor(a.type_line))
        || (a.has_line(&CHOOSE_A_BACKGROUND_LINE) && is_background(b.type_line))
        || (b.has_line(&CHOOSE_A_BACKGROUND_LINE) && is_background(a.type_line))
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
        assert!(can_be_commander("Legendary Artifact Creature — Golem", ""));
        assert!(!can_be_commander("Legendary Artifact", ""));
        assert!(can_be_commander(
            "Legendary Planeswalker — Test",
            "Test can be your commander."
        ));
        assert!(can_be_commander(
            "Legendary Enchantment",
            "Ashaya's Enduring Bond can be your commander."
        ));
        assert!(!can_be_commander("Legendary Enchantment — Background", ""));
        assert!(!can_be_commander("Creature — Cat", ""));
        assert!(!can_be_commander(
            "Legendary Planeswalker — Jace",
            "+1: Draw."
        ));
        assert!(!can_be_commander("Legendary Artifact — Equipment", ""));
    }

    /// `Manavault.Catalog.CommanderRulesTest`: CR 903.3 cases.
    #[test]
    fn accepts_legendary_vehicles_and_spacecraft_and_judges_the_front_face() {
        assert!(can_be_commander("Legendary Artifact — Vehicle", ""));
        assert!(can_be_commander("Legendary Artifact — Spacecraft", ""));
        assert!(can_be_commander(
            "Legendary Creature — God // Legendary Enchantment",
            ""
        ));
        assert!(!can_be_commander(
            "Legendary Enchantment — Saga // Legendary Creature — Snake",
            ""
        ));
        assert!(!can_be_commander(
            "Sorcery",
            "Return your commander to your hand. Vehicles can crew."
        ));
        assert!(!can_be_commander("", ""));
    }

    fn card<'a>(name: &'a str, type_line: &'a str, oracle_text: &'a str) -> CommanderCard<'a> {
        CommanderCard {
            name,
            type_line,
            oracle_text,
        }
    }

    #[test]
    fn pairs_partner_keywords_only_with_matching_labels() {
        let plain = card(
            "A",
            CREATURE,
            "Partner (You can have two commanders if both have partner.)",
        );
        let plain_2 = card("B", CREATURE, "Flying\nPartner");
        let survivors = card(
            "C",
            CREATURE,
            "Partner—Survivors (You can have two commanders if both have this ability.)",
        );
        let survivors_2 = card("D", CREATURE, "Partner — survivors");
        let partner_with = card("E", CREATURE, "Partner with B");
        assert!(valid_pair(&plain, &plain_2));
        assert!(valid_pair(&survivors, &survivors_2));
        assert!(!valid_pair(&plain, &survivors));
        assert!(!valid_pair(&plain, &partner_with));
        assert!(!valid_pair(
            &plain,
            &card("F", CREATURE, "Partners in crime")
        ));
        assert!(!valid_pair(
            &plain,
            &card("G", CREATURE, "Whenever a Partner enters, draw a card.")
        ));
    }

    #[test]
    fn pairs_partner_with_only_when_both_name_each_other() {
        let pir = card(
            "Pir, Imaginative Rascal",
            CREATURE,
            "Partner with Toothy, Imaginary Friend (When this creature enters, target player may put Toothy into their hand from their library, then shuffle.)",
        );
        let toothy = card(
            "Toothy, Imaginary Friend",
            CREATURE,
            "Partner with Pir, Imaginative Rascal\nTrample",
        );
        let impostor = card(
            "Someone Else",
            CREATURE,
            "Partner with Pir, Imaginative Rascal",
        );
        assert!(valid_pair(&pir, &toothy));
        assert!(!valid_pair(&pir, &impostor));
        assert!(!valid_pair(&toothy, &impostor));
        let mdfc = card(
            "Toothy, Imaginary Friend // Toothy, Back",
            CREATURE,
            "Partner with Pir, Imaginative Rascal",
        );
        assert!(valid_pair(&pir, &mdfc));
    }

    #[test]
    fn pairs_friends_forever_doctors_and_backgrounds() {
        let friend = card("A", CREATURE, "Friends forever");
        let friend_2 = card(
            "B",
            CREATURE,
            "Partner—Friends forever (You can have two commanders if both have this ability.)",
        );
        assert!(valid_pair(&friend, &friend_2));
        assert!(!valid_pair(&friend, &card("C", CREATURE, "Partner")));

        let companion = card(
            "D",
            "Legendary Creature — Human Advisor",
            "Doctor's companion (You can have two commanders if the other is the Doctor.)",
        );
        let doctor = card("E", "Legendary Creature — Time Lord Doctor", "Haste");
        assert!(valid_pair(&companion, &doctor));
        assert!(valid_pair(&doctor, &companion));
        assert!(!valid_pair(
            &companion,
            &card("F", "Creature — Time Lord Doctor", "")
        ));
        assert!(!valid_pair(&doctor, &doctor));

        let chooser = card(
            "G",
            CREATURE,
            "Reach\nChoose a Background (You can have a Background as a second commander.)",
        );
        let background = card(
            "H",
            "Legendary Enchantment — Background",
            "Commander creatures you own have haste.",
        );
        assert!(valid_pair(&chooser, &background));
        assert!(valid_pair(&background, &chooser));
        assert!(!valid_pair(&background, &background));
        assert!(!valid_pair(&chooser, &friend));
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
