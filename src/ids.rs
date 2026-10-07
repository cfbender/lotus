//! Scryfall identifiers.
//!
//! Scryfall uses two different UUIDs for a card: one for its rules text
//! ([`OracleId`]) and one for each printing ([`ScryfallId`]). Keeping them
//! as distinct types stops one from being passed where the other belongs.

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! string_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        #[cfg_attr(feature = "sqlx", derive(sqlx::Type), sqlx(transparent))]
        pub struct $name(String);

        impl $name {
            /// Wraps a raw identifier.
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            /// The identifier as text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }

            /// Unwraps the identifier into its text.
            #[must_use]
            pub fn into_string(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_owned())
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

string_id!(
    /// Scryfall's identifier for a card's rules text, shared by all its printings.
    OracleId
);

string_id!(
    /// Scryfall's identifier for one printing of a card.
    ScryfallId
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_serialize_as_plain_strings() {
        let id = ScryfallId::new("abc");
        assert_eq!(serde_json::to_string(&id).unwrap(), "\"abc\"");
        let back: ScryfallId = serde_json::from_str("\"abc\"").unwrap();
        assert_eq!(back, id);
        assert_eq!(id.to_string(), "abc");
    }
}
