//! Scryfall rulings (`GET /cards/:id/rulings`).

use serde::{Deserialize, Serialize};

/// One ruling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ruling {
    /// `wotc` or `scryfall`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Publication date as `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    /// The ruling text.
    pub comment: String,
}

/// The rulings list response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RulingsList {
    /// The rulings, oldest first as Scryfall sends them.
    #[serde(default)]
    pub data: Vec<Ruling>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_rulings_and_rejects_missing_comments() {
        let list: RulingsList = serde_json::from_str(
            r#"{"object":"list","data":[{"source":"wotc","published_at":"2004-10-04","comment":"It works."}]}"#,
        )
        .unwrap();
        assert_eq!(list.data[0].comment, "It works.");
        assert!(serde_json::from_str::<RulingsList>(r#"{"data":[{"source":"wotc"}]}"#).is_err());
    }
}
