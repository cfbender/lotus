//! Deck lists hosted on Moxfield, Archidekt, and ManaVault share links.
//!
//! [`DeckLink`] recognizes a pasted URL, each source module turns that
//! site's JSON into a [`Decklist`], and (with the `http` feature)
//! [`DecklistClient`] fetches one with the hardening both apps apply:
//! bounded response size and time, no redirects, and a public-destination
//! policy for ManaVault instances.

pub mod archidekt;
#[cfg(feature = "http")]
pub mod client;
pub mod destination;
pub mod link;
pub mod manavault;
pub mod moxfield;

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::card::{Color, Finish, Zone};
use crate::ids::ScryfallId;
use crate::quantity::Quantity;

pub use archidekt::ArchidektDeck;
#[cfg(feature = "http")]
pub use client::{DecklistClient, DecklistClientBuilder, Resolver, SystemResolver};
pub use destination::{Allowlist, Origin, Scheme, is_public_address};
pub use link::{DeckLink, LinkError, ShareKind, ShareLink, is_share_token};
pub use manavault::{DeckPager, Limits};
pub use moxfield::MoxfieldDeck;

/// Where a deck list came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(rename_all = "snake_case"))]
pub enum Source {
    /// moxfield.com.
    Moxfield,
    /// archidekt.com.
    Archidekt,
    /// A ManaVault instance's public share link.
    #[serde(rename = "manavault")]
    #[cfg_attr(feature = "sqlx", sqlx(rename = "manavault"))]
    ManaVault,
}

impl Source {
    /// The stored text value.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Moxfield => "moxfield",
            Self::Archidekt => "archidekt",
            Self::ManaVault => "manavault",
        }
    }

    /// Parses a stored text value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "moxfield" => Some(Self::Moxfield),
            "archidekt" => Some(Self::Archidekt),
            "manavault" => Some(Self::ManaVault),
            _ => None,
        }
    }
}

impl fmt::Display for Source {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One line of a deck list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// The card name as the source spells it.
    pub name: String,
    /// How many copies.
    pub quantity: Quantity,
    /// Which board the card is on.
    pub zone: Zone,
    /// The exact printing the source names, when it records one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scryfall_id: Option<ScryfallId>,
    /// Set code, when the source records one and no printing id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub set_code: Option<String>,
    /// Collector number, when the source records one and no printing id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collector_number: Option<String>,
    /// The finish the source records, defaulting to non-foil.
    #[serde(default)]
    pub finish: Finish,
}

impl Entry {
    /// An entry with only a name, quantity, and zone.
    #[must_use]
    pub fn new(name: impl Into<String>, quantity: Quantity, zone: Zone) -> Self {
        Self {
            name: name.into(),
            quantity,
            zone,
            scryfall_id: None,
            set_code: None,
            collector_number: None,
            finish: Finish::DEFAULT,
        }
    }

    /// Builds an entry from untrusted source fields: `None` when the name is
    /// missing or blank, and a quantity of one when the source's is missing
    /// or not positive. Blank printing ids are dropped.
    #[must_use]
    pub fn from_source(
        name: Option<String>,
        quantity: Option<i64>,
        zone: Zone,
        scryfall_id: Option<String>,
    ) -> Option<Self> {
        let name = name.filter(|name| !name.is_empty())?;
        Some(Self {
            scryfall_id: scryfall_id.filter(|id| !id.is_empty()).map(ScryfallId::new),
            ..Self::new(name, Quantity::or_one(quantity), zone)
        })
    }
}

/// A deck list resolved from a source, with the public metadata the source
/// exposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decklist {
    /// Which site served it.
    pub source: Source,
    /// The source's id for the deck (a share token for ManaVault).
    pub id: String,
    /// The canonical public URL.
    pub url: String,
    /// The deck's name, when the source sends one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The deck's author, when the source exposes one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// The source's own count of cards in the playable deck, when it sends
    /// one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_count: Option<u64>,
    /// Names of the cards in the command zone, in source order.
    #[serde(default)]
    pub commanders: Vec<String>,
    /// The commanders' combined color identity in WUBRG order, when known.
    /// Colorless commanders and sources that do not expose colors both
    /// yield an empty list.
    #[serde(default)]
    pub color_identity: Vec<Color>,
    /// Every entry, including cards under consideration.
    #[serde(default)]
    pub entries: Vec<Entry>,
}

impl Decklist {
    /// Entries in the command zone and main deck.
    pub fn playable(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|entry| entry.zone.in_deck())
    }
}

/// Why a deck list could not be fetched.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FetchError {
    /// The source has no such deck (HTTP 404 or a null GraphQL result).
    #[error("the deck was not found; it may be private or deleted")]
    NotFound,
    /// The source refused the request (HTTP 401 or 403).
    #[error("the deck site refused the request")]
    Forbidden,
    /// The source answered with another non-success status.
    #[error("the deck site returned HTTP {0}")]
    HttpStatus(u16),
    /// Connecting or reading timed out.
    #[error("timed out reaching the deck site")]
    Timeout,
    /// The request could not be made at all.
    #[error("could not reach the deck site")]
    RequestFailed,
    /// The response was larger than the configured cap.
    #[error("the deck site's response was too large")]
    BodyTooLarge,
    /// The response was not JSON.
    #[error("the deck site returned an unreadable response")]
    InvalidJson,
    /// The response was JSON of an unexpected shape.
    #[error("the deck site returned an unexpected response")]
    Malformed,
    /// The GraphQL endpoint answered with errors.
    #[error("the ManaVault instance returned errors: {}", .0.join("; "))]
    GraphqlErrors(Vec<String>),
    /// The remote ManaVault schema has no field for this share kind.
    #[error("that ManaVault instance doesn't support shared {0} lists yet")]
    Unsupported(ShareKind),
    /// The remote ManaVault predates [`manavault::MIN_SERVER_VERSION`] and
    /// rejected a field of the deck query.
    #[error("ManaVault server too old (needs v{}+)", manavault::MIN_SERVER_VERSION)]
    ServerTooOld,
    /// Pagination did not advance.
    #[error("that ManaVault instance returned invalid list pagination")]
    InvalidPagination,
    /// The import exceeded its page, entry, byte, or time budget.
    #[error("that shared list is too large or took too long to import")]
    LimitExceeded,
    /// The link's scheme or host is not one the client will request.
    #[error("unsupported link")]
    UnsupportedLink,
    /// The host resolved to an address the destination policy blocks.
    #[error("the destination resolved to a blocked network address")]
    BlockedDestination,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_from_source_drops_blank_names_and_ids() {
        assert!(Entry::from_source(None, Some(1), Zone::Mainboard, None).is_none());
        assert!(Entry::from_source(Some(String::new()), Some(1), Zone::Mainboard, None).is_none());
        let entry = Entry::from_source(
            Some("Sol Ring".into()),
            Some(0),
            Zone::Commander,
            Some(String::new()),
        )
        .unwrap();
        assert_eq!(entry.quantity, Quantity::ONE);
        assert_eq!(entry.scryfall_id, None);
        assert_eq!(entry.finish, Finish::Nonfoil);
    }

    #[test]
    fn playable_excludes_considering() {
        let list = Decklist {
            source: Source::Moxfield,
            id: "x".into(),
            url: "u".into(),
            name: None,
            author: None,
            card_count: None,
            commanders: vec![],
            color_identity: vec![],
            entries: vec![
                Entry::new("a", Quantity::ONE, Zone::Commander),
                Entry::new("b", Quantity::ONE, Zone::Considering),
                Entry::new("c", Quantity::ONE, Zone::Mainboard),
            ],
        };
        let names: Vec<&str> = list.playable().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["a", "c"]);
    }
}
