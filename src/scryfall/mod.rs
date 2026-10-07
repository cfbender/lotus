//! Scryfall's card model, bulk-data files, and (with the `http` feature) API
//! client.

pub mod bulk;
pub mod card;
pub mod catalog;
#[cfg(feature = "http")]
pub mod client;
pub mod rulings;

pub use bulk::{BULK_DATA_URL, BulkData, BulkDataList, BulkError, DEFAULT_CARDS_URL, JsonLines};
pub use card::{CardFace, ImageUris, RelatedCard, ScryfallCard, parse_date};
pub use catalog::{Exclusion, SelectionKey, describes_card, import_policy};
#[cfg(feature = "http")]
pub use client::{ScryfallClient, ScryfallError, SearchPage};
pub use rulings::{Ruling, RulingsList};
