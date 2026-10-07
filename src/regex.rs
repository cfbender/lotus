//! Compiling the crate's literal regular expressions.

use regex::Regex;

/// Compiles a pattern that is a literal in this crate.
///
/// Every caller passes a string literal, and each module's tests exercise
/// its patterns, so a compile failure is a programming error caught before
/// release. This is the one place the crate allows `expect`.
#[allow(clippy::expect_used)]
pub(crate) fn compile(pattern: &str) -> Regex {
    Regex::new(pattern).expect("literal regular expression compiles")
}
