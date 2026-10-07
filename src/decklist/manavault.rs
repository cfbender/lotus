//! Another ManaVault instance's public share GraphQL endpoint
//! (`Manavault.Trade.ListSource.ManaVaultRemote`,
//! `TheGathering.Decklists.Sources.Manavault`).
//!
//! Everything here is pure: the queries, the typed request and response
//! shapes, the import budget, and a [`DeckPager`] state machine that walks
//! the `deckCards` connection. The `http` feature's
//! [`DecklistClient`](crate::decklist::DecklistClient) drives these over
//! the network; an app with its own HTTP stack can drive them itself.
//!
//! The deck query asks for every field both apps read, so the remote
//! instance must be ManaVault [`MIN_SERVER_VERSION`] or newer: `finish`,
//! `cardCount`, and `preferredPrinting` have existed since v0.2.2,
//! `fallbackPrinting` since v0.11.0, and `commanderColorIdentity` since
//! v1.3.0, which sets the minimum. An older instance rejects the query
//! with a "Cannot query field" error, which [`DeckPager::accept`] reports
//! as [`FetchError::ServerTooOld`]; there is no fallback query.

use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::card::{Color, Finish, Zone};
use crate::decklist::link::{ShareKind, ShareLink};
use crate::decklist::{Decklist, Entry, FetchError, Source};

/// The only path ever requested on a ManaVault origin.
pub const GRAPHQL_PATH: &str = "/share/graphql";

/// The oldest ManaVault release whose share schema answers [`DECK_QUERY`].
pub const MIN_SERVER_VERSION: &str = "1.3.0";

/// Fields of [`DECK_QUERY`] that older ManaVault releases lack, newest
/// first. A GraphQL error naming one means the server predates
/// [`MIN_SERVER_VERSION`].
const VERSIONED_DECK_FIELDS: [&str; 5] = [
    "commanderColorIdentity",
    "fallbackPrinting",
    "preferredPrinting",
    "cardCount",
    "finish",
];

/// Name given to an imported want list.
pub const WANTS_NAME: &str = "Shared wants";
/// Name given to an imported trade binder.
pub const BINDER_NAME: &str = "Trade binder";

/// `deck(id:)` with one page of `deckCards`. The public schema clamps
/// `first` to 500.
pub const DECK_QUERY: &str = "\
query FetchSharedDeck($id: ID!, $after: String) {
  deck(id: $id) {
    name
    cardCount
    commanderColorIdentity
    deckCards(first: 500, after: $after) {
      pageInfo { hasNextPage endCursor }
      edges {
        node {
          quantity
          zone
          finish
          card { name }
          preferredPrinting { scryfallId }
          fallbackPrinting { scryfallId }
        }
      }
    }
  }
}
";

/// `wantsList(id:)`.
pub const WANTS_QUERY: &str = "\
query FetchSharedWants($id: ID!) {
  wantsList(id: $id) {
    entries { cardName quantity setCode collectorNumber }
  }
}
";

/// `binderList(id:)`.
pub const BINDER_QUERY: &str = "\
query FetchSharedBinder($id: ID!) {
  binderList(id: $id) {
    entries { cardName quantity setCode collectorNumber finish }
  }
}
";

/// A GraphQL request body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GraphqlRequest<V> {
    /// The query document.
    pub query: &'static str,
    /// Its variables.
    pub variables: V,
}

/// Variables for [`DECK_QUERY`]. `after` is always sent, as `null` for the
/// first page.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeckVariables {
    /// The share token.
    pub id: String,
    /// The cursor to continue from.
    pub after: Option<String>,
}

/// Variables for [`WANTS_QUERY`] and [`BINDER_QUERY`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListVariables {
    /// The share token.
    pub id: String,
}

/// A GraphQL error.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct GraphqlError {
    /// The message.
    #[serde(default)]
    pub message: Option<String>,
}

/// A GraphQL response envelope.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct GraphqlResponse<T> {
    /// The data, when the query succeeded.
    #[serde(default = "none")]
    pub data: Option<T>,
    /// Errors, when it did not.
    #[serde(default)]
    pub errors: Vec<GraphqlError>,
}

fn none<T>() -> Option<T> {
    None
}

impl<T> GraphqlResponse<T> {
    /// The data, or [`FetchError::GraphqlErrors`] when the envelope carries
    /// errors and [`FetchError::Malformed`] when it carries neither.
    pub fn into_data(self) -> Result<T, FetchError> {
        if !self.errors.is_empty() {
            return Err(FetchError::GraphqlErrors(
                self.errors
                    .into_iter()
                    .map(|error| error.message.unwrap_or_default())
                    .collect(),
            ));
        }
        self.data.ok_or(FetchError::Malformed)
    }
}

/// `data` of [`DECK_QUERY`].
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct DeckData {
    /// `null` when the token matches no deck.
    #[serde(default)]
    pub deck: Option<SharedDeck>,
}

/// A shared deck with one page of cards.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharedDeck {
    /// The deck name.
    #[serde(default)]
    pub name: Option<String>,
    /// The instance's count of cards in the deck.
    #[serde(default)]
    pub card_count: Option<u64>,
    /// The commanders' color identity as codes.
    #[serde(default)]
    pub commander_color_identity: Option<Vec<String>>,
    /// One page of deck cards.
    #[serde(default)]
    pub deck_cards: Option<DeckCardConnection>,
}

/// A Relay connection page.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckCardConnection {
    /// The page's edges.
    #[serde(default)]
    pub edges: Vec<DeckCardEdge>,
    /// Where the page ends.
    #[serde(default)]
    pub page_info: Option<PageInfo>,
}

/// Relay page info.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageInfo {
    /// Whether another page follows.
    #[serde(default)]
    pub has_next_page: bool,
    /// The cursor to continue from.
    #[serde(default)]
    pub end_cursor: Option<String>,
}

/// A connection edge.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct DeckCardEdge {
    /// The deck card.
    #[serde(default)]
    pub node: Option<DeckCardNode>,
}

/// A deck card.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckCardNode {
    /// How many copies.
    #[serde(default)]
    pub quantity: Option<i64>,
    /// The zone name; legacy `sideboard`/`maybeboard` values are accepted.
    #[serde(default)]
    pub zone: Option<String>,
    /// The finish name.
    #[serde(default)]
    pub finish: Option<String>,
    /// The Oracle card.
    #[serde(default)]
    pub card: Option<NamedCard>,
    /// The printing the owner chose.
    #[serde(default)]
    pub preferred_printing: Option<PrintingRef>,
    /// The printing the instance falls back to.
    #[serde(default)]
    pub fallback_printing: Option<PrintingRef>,
}

/// A card with only its name.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
pub struct NamedCard {
    /// The card name.
    #[serde(default)]
    pub name: Option<String>,
}

/// A printing reference.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrintingRef {
    /// The Scryfall printing id.
    #[serde(default)]
    pub scryfall_id: Option<String>,
}

impl DeckCardNode {
    /// The entry, or `None` without a card name. Unknown zones become the
    /// main deck; the preferred printing wins over the fallback.
    #[must_use]
    pub fn entry(&self) -> Option<Entry> {
        let zone = self
            .zone
            .as_deref()
            .and_then(Zone::parse)
            .unwrap_or(Zone::Mainboard);
        let printing = self
            .preferred_printing
            .as_ref()
            .and_then(|printing| printing.scryfall_id.clone())
            .filter(|id| !id.is_empty())
            .or_else(|| {
                self.fallback_printing
                    .as_ref()
                    .and_then(|printing| printing.scryfall_id.clone())
            });
        let mut entry = Entry::from_source(
            self.card.as_ref()?.name.clone(),
            self.quantity,
            zone,
            printing,
        )?;
        entry.finish = self
            .finish
            .as_deref()
            .and_then(Finish::parse)
            .unwrap_or(Finish::DEFAULT);
        Some(entry)
    }
}

/// `data` of [`WANTS_QUERY`].
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WantsData {
    /// `null` when the token matches no want list.
    #[serde(default)]
    pub wants_list: Option<SharedList>,
}

/// `data` of [`BINDER_QUERY`].
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BinderData {
    /// `null` when the token matches no binder.
    #[serde(default)]
    pub binder_list: Option<SharedList>,
}

/// A shared want list or trade binder.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct SharedList {
    /// The entries.
    #[serde(default)]
    pub entries: Vec<ListEntry>,
}

/// One want-list or binder entry.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListEntry {
    /// The card name.
    #[serde(default)]
    pub card_name: Option<String>,
    /// How many copies.
    #[serde(default)]
    pub quantity: Option<i64>,
    /// The set code, when the owner pinned a printing.
    #[serde(default)]
    pub set_code: Option<String>,
    /// The collector number, when the owner pinned a printing.
    #[serde(default)]
    pub collector_number: Option<String>,
    /// The finish (binders only).
    #[serde(default)]
    pub finish: Option<String>,
}

impl ListEntry {
    /// The entry in the main deck, or `None` without a card name.
    #[must_use]
    pub fn entry(&self) -> Option<Entry> {
        let mut entry =
            Entry::from_source(self.card_name.clone(), self.quantity, Zone::Mainboard, None)?;
        entry.set_code = self.set_code.clone().filter(|code| !code.is_empty());
        entry.collector_number = self
            .collector_number
            .clone()
            .filter(|number| !number.is_empty());
        entry.finish = self
            .finish
            .as_deref()
            .and_then(Finish::parse)
            .unwrap_or(Finish::DEFAULT);
        Some(entry)
    }
}

/// Caps on one import from another instance (ManaVault's defaults).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    /// How many GraphQL requests one import may make.
    pub max_pages: u32,
    /// How many entries one import may collect.
    pub max_entries: usize,
    /// Total response bytes across every page.
    pub max_bytes: u64,
    /// Response bytes for any one page.
    pub max_page_bytes: u64,
    /// Wall-clock budget for the whole import.
    pub timeout: Duration,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_pages: 10,
            max_entries: 10_000,
            max_bytes: 10_000_000,
            max_page_bytes: 5_000_000,
            timeout: Duration::from_secs(30),
        }
    }
}

/// What one import has consumed of its [`Limits`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Budget {
    limits: Limits,
    pages: u32,
    entries: usize,
    bytes: u64,
}

impl Budget {
    /// A fresh budget.
    #[must_use]
    pub fn new(limits: Limits) -> Self {
        Self {
            limits,
            pages: 0,
            entries: 0,
            bytes: 0,
        }
    }

    /// The most bytes the next page may have, or
    /// [`FetchError::LimitExceeded`] when no page may be requested.
    pub fn page_allowance(&self) -> Result<u64, FetchError> {
        let remaining = self.limits.max_bytes.saturating_sub(self.bytes);
        if self.pages >= self.limits.max_pages || remaining == 0 {
            return Err(FetchError::LimitExceeded);
        }
        Ok(remaining.min(self.limits.max_page_bytes))
    }

    /// Records a received page of `bytes`.
    pub fn record_page(&mut self, bytes: u64) -> Result<(), FetchError> {
        self.pages = self.pages.saturating_add(1);
        self.bytes = self.bytes.saturating_add(bytes);
        if self.bytes > self.limits.max_bytes {
            return Err(FetchError::LimitExceeded);
        }
        Ok(())
    }

    /// Records `count` collected entries.
    pub fn record_entries(&mut self, count: usize) -> Result<(), FetchError> {
        self.entries = self.entries.saturating_add(count);
        if self.entries > self.limits.max_entries {
            return Err(FetchError::LimitExceeded);
        }
        Ok(())
    }

    /// Pages received so far.
    #[must_use]
    pub fn pages(&self) -> u32 {
        self.pages
    }
}

/// Whether to request another page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// Request the page [`DeckPager::request`] now describes.
    Next,
    /// Every page has been read; call [`DeckPager::finish`].
    Done,
}

/// Walks a shared deck's `deckCards` pages, enforcing the budget and
/// rejecting pagination that does not advance.
#[derive(Debug, Clone, PartialEq)]
pub struct DeckPager {
    token: String,
    cursor: Option<String>,
    seen: HashSet<String>,
    budget: Budget,
    deck: Option<SharedDeck>,
    entries: Vec<Entry>,
}

impl DeckPager {
    /// A pager for the deck shared as `token`.
    #[must_use]
    pub fn new(token: impl Into<String>, limits: Limits) -> Self {
        Self {
            token: token.into(),
            cursor: None,
            seen: HashSet::new(),
            budget: Budget::new(limits),
            deck: None,
            entries: Vec::new(),
        }
    }

    /// The budget so far.
    #[must_use]
    pub fn budget(&self) -> &Budget {
        &self.budget
    }

    /// The request for the next page.
    #[must_use]
    pub fn request(&self) -> GraphqlRequest<DeckVariables> {
        GraphqlRequest {
            query: DECK_QUERY,
            variables: DeckVariables {
                id: self.token.clone(),
                after: self.cursor.clone(),
            },
        }
    }

    /// The most bytes the next page may have.
    pub fn page_allowance(&self) -> Result<u64, FetchError> {
        self.budget.page_allowance()
    }

    /// Accepts the response to [`Self::request`], which was `bytes` long.
    ///
    /// A `null` deck is [`FetchError::NotFound`]. A next cursor that is
    /// empty, equal to the current one, or already seen is
    /// [`FetchError::InvalidPagination`]. A GraphQL error rejecting one of
    /// the query's newer fields is [`FetchError::ServerTooOld`].
    pub fn accept(
        &mut self,
        response: GraphqlResponse<DeckData>,
        bytes: u64,
    ) -> Result<Step, FetchError> {
        self.budget.record_page(bytes)?;
        let deck = response
            .into_data()
            .map_err(classify_deck_error)?
            .deck
            .ok_or(FetchError::NotFound)?;
        let connection = deck.deck_cards.clone().ok_or(FetchError::Malformed)?;
        let page: Vec<Entry> = connection
            .edges
            .iter()
            .filter_map(|edge| edge.node.as_ref()?.entry())
            .collect();
        self.budget.record_entries(page.len())?;
        self.entries.extend(page);
        if self.deck.is_none() {
            self.deck = Some(deck);
        }
        let page_info = connection.page_info.unwrap_or_default();
        if !page_info.has_next_page {
            return Ok(Step::Done);
        }
        let next = page_info
            .end_cursor
            .filter(|cursor| !cursor.is_empty())
            .ok_or(FetchError::InvalidPagination)?;
        if self.cursor.as_ref() == Some(&next) || !self.seen.insert(next.clone()) {
            return Err(FetchError::InvalidPagination);
        }
        self.cursor = Some(next);
        Ok(Step::Next)
    }

    /// The collected deck as a [`Decklist`] whose `url` is `canonical_url`.
    /// Commanders are the entries in the command zone, in deck order.
    #[must_use]
    pub fn finish(self, canonical_url: impl Into<String>) -> Decklist {
        let deck = self.deck.unwrap_or_default();
        Decklist {
            source: Source::ManaVault,
            id: self.token,
            url: canonical_url.into(),
            name: deck.name,
            author: None,
            card_count: deck.card_count,
            commanders: self
                .entries
                .iter()
                .filter(|entry| entry.zone == Zone::Commander)
                .map(|entry| entry.name.clone())
                .collect(),
            color_identity: Color::identity(
                deck.commander_color_identity
                    .iter()
                    .flatten()
                    .filter_map(|code| Color::parse(code)),
            ),
            entries: self.entries,
        }
    }
}

/// The request for a want list or trade binder.
#[must_use]
pub fn list_request(kind: ShareKind, token: &str) -> Option<GraphqlRequest<ListVariables>> {
    let query = match kind {
        ShareKind::Wants => WANTS_QUERY,
        ShareKind::Binder => BINDER_QUERY,
        ShareKind::Deck => return None,
    };
    Some(GraphqlRequest {
        query,
        variables: ListVariables {
            id: token.to_owned(),
        },
    })
}

/// The GraphQL field a share kind queries.
fn list_field(kind: ShareKind) -> &'static str {
    match kind {
        ShareKind::Deck => "deck",
        ShareKind::Wants => "wantsList",
        ShareKind::Binder => "binderList",
    }
}

/// Maps a GraphQL error from the deck query: an error naming one of the
/// fields added after the first share schema means the server predates
/// [`MIN_SERVER_VERSION`] ([`FetchError::ServerTooOld`]).
#[must_use]
pub fn classify_deck_error(error: FetchError) -> FetchError {
    match error {
        FetchError::GraphqlErrors(messages)
            if messages.iter().any(|message| {
                VERSIONED_DECK_FIELDS
                    .iter()
                    .any(|field| message.contains(field))
            }) =>
        {
            FetchError::ServerTooOld
        }
        other => other,
    }
}

/// Maps a GraphQL error from a list query: an error mentioning the field
/// means the remote schema predates it ([`FetchError::Unsupported`]).
#[must_use]
pub fn classify_list_error(kind: ShareKind, error: FetchError) -> FetchError {
    match error {
        FetchError::GraphqlErrors(messages)
            if messages
                .iter()
                .any(|message| message.contains(list_field(kind))) =>
        {
            FetchError::Unsupported(kind)
        }
        other => other,
    }
}

/// Turns a want-list or binder response into a [`Decklist`] whose entries
/// are all in the main deck, enforcing the entry limit.
pub fn list_decklist(
    share: &ShareLink,
    canonical_url: impl Into<String>,
    list: Option<SharedList>,
    limits: Limits,
) -> Result<Decklist, FetchError> {
    let list = list.ok_or(FetchError::NotFound)?;
    let entries: Vec<Entry> = list.entries.iter().filter_map(ListEntry::entry).collect();
    Budget::new(limits).record_entries(entries.len())?;
    let name = match share.kind {
        ShareKind::Wants => WANTS_NAME,
        ShareKind::Binder => BINDER_NAME,
        ShareKind::Deck => return Err(FetchError::Malformed),
    };
    Ok(Decklist {
        source: Source::ManaVault,
        id: share.token.clone(),
        url: canonical_url.into(),
        name: Some(name.to_owned()),
        author: None,
        card_count: None,
        commanders: Vec::new(),
        color_identity: Vec::new(),
        entries,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quantity::Quantity;

    fn page(
        edges: &[serde_json::Value],
        has_next: bool,
        cursor: Option<&str>,
    ) -> GraphqlResponse<DeckData> {
        serde_json::from_value(serde_json::json!({
            "data": {"deck": {
                "name": "Paged",
                "cardCount": 32,
                "commanderColorIdentity": ["U", "W"],
                "deckCards": {
                    "pageInfo": {"hasNextPage": has_next, "endCursor": cursor},
                    "edges": edges
                }
            }}
        }))
        .unwrap()
    }

    fn node(zone: &str, name: &str, quantity: i64) -> serde_json::Value {
        serde_json::json!({"node": {"quantity": quantity, "zone": zone, "card": {"name": name}}})
    }

    #[test]
    fn pager_follows_cursors_and_collects_metadata() {
        let mut pager = DeckPager::new("tok", Limits::default());
        assert_eq!(
            pager.request().variables,
            DeckVariables {
                id: "tok".into(),
                after: None
            }
        );
        assert_eq!(
            serde_json::to_value(pager.request()).unwrap()["variables"],
            serde_json::json!({"id": "tok", "after": null}),
            "the first page sends an explicit null cursor"
        );

        let first = page(
            &[node("commander", "Shorikai, Genesis Engine", 1)],
            true,
            Some("c1"),
        );
        assert_eq!(pager.accept(first, 100).unwrap(), Step::Next);
        assert_eq!(pager.request().variables.after.as_deref(), Some("c1"));

        let second = page(
            &[
                node("mainboard", "Sol Ring", 1),
                node("mainboard", "Island", 30),
            ],
            false,
            Some("c2"),
        );
        assert_eq!(pager.accept(second, 100).unwrap(), Step::Done);
        assert_eq!(pager.budget().pages(), 2);

        let deck = pager.finish("https://vault.example/share/decks/tok");
        assert_eq!(deck.name.as_deref(), Some("Paged"));
        assert_eq!(deck.card_count, Some(32));
        assert_eq!(deck.color_identity, vec![Color::W, Color::U]);
        assert_eq!(deck.commanders, vec!["Shorikai, Genesis Engine"]);
        let names: Vec<(&str, u32)> = deck
            .entries
            .iter()
            .map(|entry| (entry.name.as_str(), entry.quantity.get()))
            .collect();
        assert_eq!(
            names,
            vec![
                ("Shorikai, Genesis Engine", 1),
                ("Sol Ring", 1),
                ("Island", 30)
            ]
        );
    }

    #[test]
    fn pager_rejects_pagination_that_does_not_advance() {
        for cursor in [Some(""), None] {
            let mut pager = DeckPager::new("tok", Limits::default());
            assert_eq!(
                pager.accept(page(&[], true, cursor), 1),
                Err(FetchError::InvalidPagination),
                "{cursor:?}"
            );
        }
        let mut pager = DeckPager::new("tok", Limits::default());
        assert_eq!(pager.accept(page(&[], true, Some("c1")), 1), Ok(Step::Next));
        assert_eq!(
            pager.accept(page(&[], true, Some("c1")), 1),
            Err(FetchError::InvalidPagination),
            "a repeated cursor"
        );
    }

    #[test]
    fn pager_enforces_limits() {
        let limits = Limits {
            max_pages: 2,
            max_entries: 3,
            max_bytes: 250,
            max_page_bytes: 200,
            timeout: Duration::from_secs(1),
        };
        let mut pager = DeckPager::new("tok", limits);
        assert_eq!(pager.page_allowance(), Ok(200));
        assert_eq!(
            pager.accept(page(&[], true, Some("c1")), 100),
            Ok(Step::Next)
        );
        assert_eq!(
            pager.page_allowance(),
            Ok(150),
            "the per-page cap shrinks to the remaining total"
        );
        assert_eq!(
            pager.accept(page(&[], true, Some("c2")), 100),
            Ok(Step::Next)
        );
        assert_eq!(
            pager.page_allowance(),
            Err(FetchError::LimitExceeded),
            "page cap"
        );

        let mut pager = DeckPager::new("tok", limits);
        assert_eq!(
            pager.accept(page(&[], true, Some("c1")), 251),
            Err(FetchError::LimitExceeded),
            "byte cap"
        );

        let mut pager = DeckPager::new("tok", limits);
        let many: Vec<serde_json::Value> = (0..4)
            .map(|i| node("mainboard", &format!("Card {i}"), 1))
            .collect();
        assert_eq!(
            pager.accept(page(&many, false, None), 1),
            Err(FetchError::LimitExceeded),
            "entry cap"
        );
    }

    #[test]
    fn null_deck_and_errors() {
        let mut pager = DeckPager::new("tok", Limits::default());
        let missing: GraphqlResponse<DeckData> =
            serde_json::from_value(serde_json::json!({"data": {"deck": null}})).unwrap();
        assert_eq!(pager.accept(missing, 1), Err(FetchError::NotFound));

        let errored: GraphqlResponse<DeckData> = serde_json::from_value(
            serde_json::json!({"data": null, "errors": [{"message": "boom"}, {}]}),
        )
        .unwrap();
        assert_eq!(
            errored.into_data().map(|_| ()),
            Err(FetchError::GraphqlErrors(vec![
                "boom".into(),
                String::new()
            ]))
        );

        let empty: GraphqlResponse<DeckData> =
            serde_json::from_value(serde_json::json!({"other": 1})).unwrap();
        assert_eq!(empty.into_data().map(|_| ()), Err(FetchError::Malformed));

        let no_connection: GraphqlResponse<DeckData> =
            serde_json::from_value(serde_json::json!({"data": {"deck": {"name": "x"}}})).unwrap();
        assert_eq!(pager.accept(no_connection, 1), Err(FetchError::Malformed));
    }

    #[test]
    fn node_entries_use_preferred_printing_and_legacy_zones() {
        let node: DeckCardNode = serde_json::from_value(serde_json::json!({
            "quantity": 2, "zone": "maybeboard", "finish": "foil",
            "card": {"name": "Rhystic Study"},
            "preferredPrinting": {"scryfallId": ""},
            "fallbackPrinting": {"scryfallId": "fb"}
        }))
        .unwrap();
        let entry = node.entry().unwrap();
        assert_eq!(entry.zone, Zone::Considering);
        assert_eq!(entry.finish, Finish::Foil);
        assert_eq!(entry.quantity, Quantity::new(2).unwrap());
        assert_eq!(
            entry
                .scryfall_id
                .as_ref()
                .map(ToString::to_string)
                .as_deref(),
            Some("fb")
        );

        let unknown_zone: DeckCardNode =
            serde_json::from_value(serde_json::json!({"zone": "weird", "card": {"name": "x"}}))
                .unwrap();
        assert_eq!(unknown_zone.entry().unwrap().zone, Zone::Mainboard);
        let nameless: DeckCardNode =
            serde_json::from_value(serde_json::json!({"quantity": 1})).unwrap();
        assert_eq!(nameless.entry(), None);
    }

    #[test]
    fn list_responses() {
        let share = ShareLink {
            kind: ShareKind::Binder,
            token: "tok".into(),
        };
        let list: SharedList = serde_json::from_value(serde_json::json!({"entries": [
            {"cardName": "Sol Ring", "quantity": 3, "setCode": "c21", "collectorNumber": "263", "finish": "etched"},
            {"cardName": "Island", "quantity": 0, "setCode": "", "collectorNumber": null},
            {"quantity": 1}
        ]}))
        .unwrap();
        let deck = list_decklist(&share, "u", Some(list), Limits::default()).unwrap();
        assert_eq!(deck.name.as_deref(), Some(BINDER_NAME));
        assert_eq!(deck.id, "tok");
        assert_eq!(deck.entries.len(), 2);
        assert_eq!(deck.entries[0].set_code.as_deref(), Some("c21"));
        assert_eq!(deck.entries[0].finish, Finish::Etched);
        assert_eq!(deck.entries[1].quantity, Quantity::ONE);
        assert_eq!(deck.entries[1].set_code, None);
        assert!(
            deck.entries
                .iter()
                .all(|entry| entry.zone == Zone::Mainboard)
        );

        assert_eq!(
            list_decklist(&share, "u", None, Limits::default()).map(|_| ()),
            Err(FetchError::NotFound)
        );
        let wants = ShareLink {
            kind: ShareKind::Wants,
            token: "tok".into(),
        };
        assert_eq!(
            list_decklist(&wants, "u", Some(SharedList::default()), Limits::default())
                .unwrap()
                .name
                .as_deref(),
            Some(WANTS_NAME)
        );
        assert_eq!(list_request(ShareKind::Deck, "tok"), None);
        assert_eq!(
            list_request(ShareKind::Wants, "tok").unwrap().query,
            WANTS_QUERY
        );
    }

    #[test]
    fn old_servers_rejecting_the_deck_query_are_reported_as_too_old() {
        let too_old: GraphqlResponse<DeckData> = serde_json::from_value(serde_json::json!({
            "errors": [{"message": "Cannot query field \"commanderColorIdentity\" on type \"Deck\"."}]
        }))
        .unwrap();
        let mut pager = DeckPager::new("tok", Limits::default());
        assert_eq!(pager.accept(too_old, 10), Err(FetchError::ServerTooOld));

        let unrelated: GraphqlResponse<DeckData> = serde_json::from_value(serde_json::json!({
            "errors": [{"message": "rate limited"}]
        }))
        .unwrap();
        let mut pager = DeckPager::new("tok", Limits::default());
        assert_eq!(
            pager.accept(unrelated, 10),
            Err(FetchError::GraphqlErrors(vec!["rate limited".into()]))
        );
        assert_eq!(
            classify_deck_error(FetchError::Timeout),
            FetchError::Timeout
        );
        assert!(
            FetchError::ServerTooOld
                .to_string()
                .contains(MIN_SERVER_VERSION)
        );
    }

    #[test]
    fn unsupported_fields_are_classified() {
        let error = FetchError::GraphqlErrors(vec![
            "Cannot query field \"wantsList\" on type \"RootQueryType\"".into(),
        ]);
        assert_eq!(
            classify_list_error(ShareKind::Wants, error.clone()),
            FetchError::Unsupported(ShareKind::Wants)
        );
        assert_eq!(classify_list_error(ShareKind::Binder, error.clone()), error);
        assert_eq!(
            classify_list_error(ShareKind::Wants, FetchError::Timeout),
            FetchError::Timeout
        );
    }
}
