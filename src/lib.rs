//! Magic: The Gathering domain code shared by
//! [ManaVault](https://github.com/cfbender/manavault) and
//! [the-gathering](https://github.com/cfbender/the-gathering).
//!
//! The crate has no app-specific schema. It covers:
//!
//! - [`card`], [`ids`], [`quantity`], [`name`], and [`commander`]: the
//!   vocabulary both apps store and reason about, with invalid values made
//!   unrepresentable.
//! - [`scryfall`]: Scryfall's card model, bulk-data files, catalog import
//!   policy, and (feature `http`) an API client.
//! - [`decklist`]: pasted Moxfield, Archidekt, and ManaVault share links,
//!   the JSON each site serves, and (feature `http`) a hardened fetcher.
//!
//! Feature `sqlx` adds `sqlx::Type` impls so the newtypes and enums map to
//! SQLite columns directly.

pub mod card;
pub mod commander;
pub mod decklist;
pub mod ids;
pub mod name;
pub mod quantity;
pub(crate) mod regex;
pub mod scryfall;

pub use card::{Color, Condition, Finish, Game, Legality, Rarity, Zone, is_basic_land};
pub use commander::{CommanderPairing, can_be_commander, commander_pairing};
pub use ids::{OracleId, ScryfallId};
pub use name::{match_key, normalize_name};
pub use quantity::Quantity;
