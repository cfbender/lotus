//! Recognizing pasted deck links.

use std::fmt;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::decklist::destination::Origin;
use crate::regex::compile;

static MOXFIELD_ID: LazyLock<Regex> = LazyLock::new(|| compile(r"^[A-Za-z0-9_-]{5,64}$"));
static ARCHIDEKT_ID: LazyLock<Regex> = LazyLock::new(|| compile(r"^\d{1,12}$"));
static SHARE_TOKEN: LazyLock<Regex> = LazyLock::new(|| compile(r"^[A-Za-z0-9_-]{24}$"));

const MOXFIELD_HOSTS: [&str; 2] = ["moxfield.com", "www.moxfield.com"];
const ARCHIDEKT_HOSTS: [&str; 2] = ["archidekt.com", "www.archidekt.com"];

/// What a ManaVault share link points at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShareKind {
    /// `/share/decks/<token>`.
    Deck,
    /// `/share/wants/<token>`.
    Wants,
    /// `/share/binder/<token>`.
    Binder,
}

impl ShareKind {
    /// The path segment after `/share/`.
    #[must_use]
    pub fn path_segment(self) -> &'static str {
        match self {
            Self::Deck => "decks",
            Self::Wants => "wants",
            Self::Binder => "binder",
        }
    }

    fn from_segment(segment: &str) -> Option<Self> {
        match segment {
            "decks" => Some(Self::Deck),
            "wants" => Some(Self::Wants),
            "binder" => Some(Self::Binder),
            _ => None,
        }
    }
}

impl fmt::Display for ShareKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Deck => "deck",
            Self::Wants => "want",
            Self::Binder => "binder",
        })
    }
}

/// A ManaVault share path: the kind of list and its token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ShareLink {
    /// Which list the link shares.
    pub kind: ShareKind,
    /// The share token, percent-decoded.
    pub token: String,
}

impl ShareLink {
    /// Parses `/share/decks/<token>`, `/share/wants/<token>`, or
    /// `/share/binder/<token>`, with an optional trailing slash. Query and
    /// fragment must already be removed.
    #[must_use]
    pub fn parse_path(path: &str) -> Option<Self> {
        let rest = path.strip_prefix("/share/")?;
        let (segment, token) = rest.split_once('/')?;
        let kind = ShareKind::from_segment(segment)?;
        let token = token.strip_suffix('/').unwrap_or(token);
        if token.is_empty() || token.contains(['/', '?', '#']) {
            return None;
        }
        let token = percent_decode(token);
        Some(Self { kind, token })
    }

    /// The share path.
    #[must_use]
    pub fn path(&self) -> String {
        format!("/share/{}/{}", self.kind.path_segment(), self.token)
    }
}

fn percent_decode(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}

/// Whether `token` has the shape ManaVault generates: 24 URL-safe base64
/// characters encoding 18 random bytes.
#[must_use]
pub fn is_share_token(token: &str) -> bool {
    SHARE_TOKEN.is_match(token)
}

/// Why a string is not a deck link.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LinkError {
    /// Not an absolute `http(s)` URL with a host, nor a share path.
    #[error("not a valid deck link")]
    Invalid,
}

/// A recognized deck link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeckLink {
    /// A Moxfield deck.
    Moxfield {
        /// The validated deck id.
        id: String,
    },
    /// An Archidekt deck.
    Archidekt {
        /// The validated numeric deck id.
        id: String,
    },
    /// A ManaVault share link. `origin` is `None` for a host-less path such
    /// as `/share/decks/<token>`, which the receiving instance resolves
    /// locally.
    ManaVault {
        /// The instance's scheme, host, and port.
        origin: Option<Origin>,
        /// The shared list.
        share: ShareLink,
    },
    /// A valid URL on a site this crate does not know.
    Other {
        /// The URL without its fragment.
        url: Url,
    },
}

impl DeckLink {
    /// Recognizes a pasted link. Leading and trailing whitespace is ignored.
    ///
    /// Moxfield and Archidekt links need `/decks/<id>` with a well-formed id
    /// (`[A-Za-z0-9_-]{5,64}` and `\d{1,12}` respectively); a trailing slug
    /// is ignored. Any other host with a share path is a ManaVault link, as
    /// is a bare share path. Anything else that is still an absolute
    /// `http(s)` URL is [`DeckLink::Other`].
    pub fn parse(input: &str) -> Result<Self, LinkError> {
        let input = input.trim();
        if input.starts_with('/') {
            let path = input.split(['?', '#']).next().unwrap_or(input);
            return ShareLink::parse_path(path)
                .map(|share| Self::ManaVault {
                    origin: None,
                    share,
                })
                .ok_or(LinkError::Invalid);
        }
        let url = Url::parse(input).map_err(|_| LinkError::Invalid)?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(LinkError::Invalid);
        }
        let host = url
            .host_str()
            .ok_or(LinkError::Invalid)?
            .to_ascii_lowercase();
        if MOXFIELD_HOSTS.contains(&host.as_str()) {
            if let Some(id) = deck_id(url.path(), &MOXFIELD_ID) {
                return Ok(Self::Moxfield { id });
            }
        } else if ARCHIDEKT_HOSTS.contains(&host.as_str()) {
            if let Some(id) = deck_id(url.path(), &ARCHIDEKT_ID) {
                return Ok(Self::Archidekt { id });
            }
        } else if let Some(share) = ShareLink::parse_path(url.path()) {
            let origin = Origin::of(&url).ok_or(LinkError::Invalid)?;
            return Ok(Self::ManaVault {
                origin: Some(origin),
                share,
            });
        }
        let mut url = url;
        url.set_fragment(None);
        Ok(Self::Other { url })
    }

    /// Whether the host is Moxfield's.
    #[must_use]
    pub fn is_moxfield_host(host: &str) -> bool {
        MOXFIELD_HOSTS.contains(&host.to_ascii_lowercase().as_str())
    }

    /// Whether the host is Archidekt's.
    #[must_use]
    pub fn is_archidekt_host(host: &str) -> bool {
        ARCHIDEKT_HOSTS.contains(&host.to_ascii_lowercase().as_str())
    }

    /// The canonical public URL: `https://moxfield.com/decks/<id>`,
    /// `https://archidekt.com/decks/<id>`, `<origin>/share/<kind>/<token>`,
    /// the bare share path, or the other URL without its fragment.
    #[must_use]
    pub fn canonical_url(&self) -> String {
        match self {
            Self::Moxfield { id } => format!("https://moxfield.com/decks/{id}"),
            Self::Archidekt { id } => format!("https://archidekt.com/decks/{id}"),
            Self::ManaVault { origin, share } => match origin {
                Some(origin) => format!("{origin}{}", share.path()),
                None => share.path(),
            },
            Self::Other { url } => url.to_string(),
        }
    }

    /// The source's own id: the deck id, or the share token.
    #[must_use]
    pub fn id(&self) -> &str {
        match self {
            Self::Moxfield { id } | Self::Archidekt { id } => id,
            Self::ManaVault { share, .. } => &share.token,
            Self::Other { url } => url.as_str(),
        }
    }
}

fn deck_id(path: &str, pattern: &Regex) -> Option<String> {
    let mut segments = path.split('/').filter(|segment| !segment.is_empty());
    if segments.next()? != "decks" {
        return None;
    }
    let id = segments.next()?;
    pattern.is_match(id).then(|| id.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> DeckLink {
        DeckLink::parse(input).unwrap()
    }

    #[test]
    fn moxfield_links() {
        assert_eq!(
            parse("https://www.moxfield.com/decks/AbC123-_?x=1"),
            DeckLink::Moxfield {
                id: "AbC123-_".into()
            }
        );
        assert_eq!(
            parse(" https://MOXFIELD.com/decks/AbC123/my-deck-name/ "),
            DeckLink::Moxfield {
                id: "AbC123".into()
            }
        );
        assert_eq!(
            parse("https://moxfield.com/decks/a-b_C/primer/").canonical_url(),
            "https://moxfield.com/decks/a-b_C"
        );
        for bad in [
            "/decks/ab",
            "/decks/has%20space",
            "/users/someone",
            "/decks",
        ] {
            assert!(
                matches!(
                    parse(&format!("https://moxfield.com{bad}")),
                    DeckLink::Other { .. }
                ),
                "{bad}"
            );
        }
        let long = "a".repeat(65);
        assert!(matches!(
            parse(&format!("https://moxfield.com/decks/{long}")),
            DeckLink::Other { .. }
        ));
    }

    #[test]
    fn archidekt_links() {
        assert_eq!(
            parse("https://archidekt.com/decks/1234567/my-deck-name"),
            DeckLink::Archidekt {
                id: "1234567".into()
            }
        );
        assert_eq!(
            parse("https://www.archidekt.com/decks/456/?foo=bar").canonical_url(),
            "https://archidekt.com/decks/456"
        );
        assert!(matches!(
            parse("https://archidekt.com/decks/abc123"),
            DeckLink::Other { .. }
        ));
        assert!(matches!(
            parse("https://archidekt.com/decks/1111111111111"),
            DeckLink::Other { .. }
        ));
        assert!(!DeckLink::is_archidekt_host("archidekt.com.evil.example"));
        assert!(DeckLink::is_moxfield_host("MOXFIELD.COM"));
    }

    #[test]
    fn manavault_links() {
        let link = parse(
            "https://www.manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx/?view=grid",
        );
        assert_eq!(
            link,
            DeckLink::ManaVault {
                origin: Some(Origin::parse("https://www.manavault.example.com").unwrap()),
                share: ShareLink {
                    kind: ShareKind::Deck,
                    token: "AbCdEfGhIjKlMnOpQrStUvWx".into()
                }
            }
        );
        assert_eq!(
            link.canonical_url(),
            "https://www.manavault.example.com/share/decks/AbCdEfGhIjKlMnOpQrStUvWx"
        );
        assert_eq!(link.id(), "AbCdEfGhIjKlMnOpQrStUvWx");

        let wants = parse("http://friend.home:4000/share/wants/abc%20d");
        assert_eq!(
            wants,
            DeckLink::ManaVault {
                origin: Some(Origin::parse("http://friend.home:4000").unwrap()),
                share: ShareLink {
                    kind: ShareKind::Wants,
                    token: "abc d".into()
                }
            }
        );
        assert_eq!(
            wants.canonical_url(),
            "http://friend.home:4000/share/wants/abc d"
        );
    }

    #[test]
    fn relative_share_paths() {
        assert_eq!(
            parse("/share/binder/abcDEF-123_456"),
            DeckLink::ManaVault {
                origin: None,
                share: ShareLink {
                    kind: ShareKind::Binder,
                    token: "abcDEF-123_456".into()
                }
            }
        );
        assert_eq!(
            parse("/share/decks/abc/").canonical_url(),
            "/share/decks/abc"
        );
        for bad in [
            "/decks/abc",
            "/share/decks/",
            "/share/decks",
            "/share/wants/",
            "/share/binder",
            "/share/decks/a/b",
        ] {
            assert_eq!(DeckLink::parse(bad), Err(LinkError::Invalid), "{bad}");
        }
    }

    #[test]
    fn other_and_invalid_links() {
        assert_eq!(
            parse("http://example.com/a?b=1#section").canonical_url(),
            "http://example.com/a?b=1"
        );
        assert_eq!(DeckLink::parse("not a URL"), Err(LinkError::Invalid));
        assert_eq!(
            DeckLink::parse("ftp://moxfield.com/decks/abcde"),
            Err(LinkError::Invalid)
        );
        assert_eq!(DeckLink::parse(""), Err(LinkError::Invalid));
    }

    #[test]
    fn share_tokens() {
        assert!(is_share_token("AbCdEfGhIjKlMnOpQrStUvWx"));
        assert!(!is_share_token("abc"));
        assert!(!is_share_token("AbCdEfGhIjKlMnOpQrStUvW+"));
    }
}
