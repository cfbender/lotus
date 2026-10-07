//! Card-name normalization for matching and storage.
//!
//! Both apps store a normalized copy of every card name so SQLite never has
//! to fold case or diacritics itself. This module is the single definition
//! of that normalization (`Manavault.Catalog.Search.NameMatch.sql_normalize/1`).

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Decomposes `value`, drops combining marks, and lowercases it, so
/// `"Lim-Dûl"` becomes `"lim-dul"`.
#[must_use]
pub fn fold_diacritics(value: &str) -> String {
    value
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .collect::<String>()
        .to_lowercase()
}

fn is_apostrophe(c: char) -> bool {
    c == '\'' || c == '\u{2019}'
}

/// SQL-compatible card-name normalization: fold diacritics, lowercase, drop
/// apostrophes so `"Aurelia's"` collapses to `"aurelias"`, and squash
/// whitespace runs to a single space.
#[must_use]
pub fn normalize_name(value: &str) -> String {
    let folded = fold_diacritics(value);
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for c in folded.chars() {
        if is_apostrophe(c) {
            continue;
        }
        if c.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    out
}

/// Full normalization for in-memory matching: like [`normalize_name`], but
/// every run of non-alphanumeric characters becomes a single space, so
/// `"Fire // Ice"` becomes `"fire ice"`.
#[must_use]
pub fn match_key(value: &str) -> String {
    let folded = fold_diacritics(value);
    let mut out = String::with_capacity(folded.len());
    let mut pending_space = false;
    for c in folded.chars() {
        if is_apostrophe(c) {
            continue;
        }
        if !c.is_alphanumeric() {
            pending_space = true;
            continue;
        }
        if pending_space && !out.is_empty() {
            out.push(' ');
        }
        pending_space = false;
        out.push(c);
    }
    out
}

/// The tokens of [`match_key`].
#[must_use]
pub fn tokens(value: &str) -> Vec<String> {
    match_key(value)
        .split(' ')
        .filter(|token| !token.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_diacritics_and_case() {
        assert_eq!(fold_diacritics("Lim-Dûl's Vault"), "lim-dul's vault");
        assert_eq!(fold_diacritics("Æther Vial"), "æther vial");
        assert_eq!(fold_diacritics("Jötun Grunt"), "jotun grunt");
    }

    #[test]
    fn normalize_name_drops_apostrophes_and_squashes_whitespace() {
        assert_eq!(normalize_name("  Aurelia’s   Fury "), "aurelias fury");
        assert_eq!(normalize_name("Fire // Ice"), "fire // ice");
        assert_eq!(normalize_name("Lim-Dûl's Vault"), "lim-duls vault");
        assert_eq!(normalize_name(""), "");
    }

    #[test]
    fn match_key_collapses_punctuation() {
        assert_eq!(match_key("Fire // Ice"), "fire ice");
        assert_eq!(match_key("Mask of Memory!"), "mask of memory");
        assert_eq!(match_key("Aurelia's Fury"), "aurelias fury");
        assert_eq!(tokens("Lim-Dûl's Vault"), vec!["lim", "duls", "vault"]);
    }
}
