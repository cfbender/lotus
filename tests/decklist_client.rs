//! `DecklistClient` against a local mock server (feature `http`): the HTTP
//! error mapping, body cap, ManaVault pagination and budget, and the
//! destination policy.

#![cfg(feature = "http")]

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use lotus::card::Zone;
use lotus::decklist::manavault::Limits;
use lotus::decklist::{Allowlist, DeckLink, DecklistClient, FetchError, Resolver, ShareKind};
use wiremock::matchers::{body_partial_json, header, method, path};
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

const MOXFIELD_PARTNER: &[u8] = include_bytes!("fixtures/decklists/moxfield_partner.json");
const ARCHIDEKT_BACKGROUND: &[u8] = include_bytes!("fixtures/decklists/archidekt_background.json");

fn fixture(bytes: &[u8]) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(bytes, "application/json")
}

/// Answers every host with a fixed address list.
struct FixedResolver(Vec<IpAddr>);

impl Resolver for FixedResolver {
    fn resolve<'a>(
        &'a self,
        _host: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send + 'a>> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

fn loopback() -> IpAddr {
    IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)
}

fn builder(server: &MockServer) -> lotus::decklist::DecklistClientBuilder {
    DecklistClient::builder("lotus tests")
        .moxfield_api_base(format!("{}/moxfield/", server.uri()))
        .archidekt_api_base(format!("{}/archidekt/", server.uri()))
        .allowlist(Allowlist::parse(["127.0.0.1", "friend.home"]))
        .resolver(Arc::new(FixedResolver(vec![loopback()])))
}

fn json(value: serde_json::Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(value)
}

#[tokio::test]
async fn fetches_moxfield_and_archidekt_decks() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/moxfield/partners"))
        .and(header("accept", "application/json"))
        .respond_with(fixture(MOXFIELD_PARTNER))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/archidekt/24907541/"))
        .respond_with(fixture(ARCHIDEKT_BACKGROUND))
        .mount(&server)
        .await;
    let client = builder(&server).build().unwrap();

    let moxfield = client
        .fetch(&DeckLink::parse("https://moxfield.com/decks/partners").unwrap())
        .await
        .unwrap();
    assert_eq!(moxfield.name.as_deref(), Some("Partner Commander"));
    assert_eq!(moxfield.card_count, Some(100));

    let archidekt = client
        .fetch(&DeckLink::parse("https://archidekt.com/decks/24907541/slug").unwrap())
        .await
        .unwrap();
    assert_eq!(archidekt.author.as_deref(), Some("Will3545"));
    assert_eq!(archidekt.commanders.len(), 2);
}

#[tokio::test]
async fn maps_http_failures() {
    let server = MockServer::start().await;
    Mock::given(path("/moxfield/missing"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&server)
        .await;
    Mock::given(path("/moxfield/private"))
        .respond_with(ResponseTemplate::new(403))
        .mount(&server)
        .await;
    Mock::given(path("/moxfield/unauth"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;
    Mock::given(path("/moxfield/broken"))
        .respond_with(ResponseTemplate::new(502))
        .mount(&server)
        .await;
    Mock::given(path("/moxfield/notjson"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>"))
        .mount(&server)
        .await;
    Mock::given(path("/moxfield/wrongshape"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!(["list"])))
        .mount(&server)
        .await;
    Mock::given(path("/moxfield/redirect"))
        .respond_with(ResponseTemplate::new(302).insert_header("location", "/moxfield/partners"))
        .mount(&server)
        .await;
    Mock::given(path("/moxfield/slowww"))
        .respond_with(json(serde_json::json!({})).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    let client = builder(&server)
        .timeout(Duration::from_millis(200))
        .build()
        .unwrap();

    for (id, expected) in [
        ("missing", FetchError::NotFound),
        ("private", FetchError::Forbidden),
        ("unauth", FetchError::Forbidden),
        ("broken", FetchError::HttpStatus(502)),
        ("notjson", FetchError::InvalidJson),
        ("wrongshape", FetchError::Malformed),
        ("redirect", FetchError::HttpStatus(302)),
        ("slowww", FetchError::Timeout),
    ] {
        assert_eq!(client.fetch_moxfield(id).await, Err(expected), "{id}");
    }
}

#[tokio::test]
async fn caps_response_bodies_without_trusting_content_length() {
    let server = MockServer::start().await;
    let big = serde_json::json!({"name": "x".repeat(10_000)});
    Mock::given(path("/moxfield/bigbig"))
        .respond_with(json(big))
        .mount(&server)
        .await;
    let client = builder(&server).max_bytes(1_000).build().unwrap();
    assert_eq!(
        client.fetch_moxfield("bigbig").await,
        Err(FetchError::BodyTooLarge)
    );
}

#[tokio::test]
async fn unsupported_links_make_no_request() {
    let server = MockServer::start().await;
    let client = builder(&server).build().unwrap();
    assert_eq!(
        client
            .fetch(&DeckLink::parse("/share/decks/local").unwrap())
            .await,
        Err(FetchError::UnsupportedLink)
    );
    assert_eq!(
        client
            .fetch(&DeckLink::parse("https://example.com/deck").unwrap())
            .await,
        Err(FetchError::UnsupportedLink)
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

fn deck_page(edges: &[serde_json::Value], cursor: Option<&str>) -> serde_json::Value {
    serde_json::json!({"data": {"deck": {
        "name": "Paged",
        "cardCount": 32,
        "commanderColorIdentity": ["W", "U"],
        "deckCards": {
            "pageInfo": {"hasNextPage": cursor.is_some(), "endCursor": cursor},
            "edges": edges
        }
    }}})
}

fn node(zone: &str, name: &str, quantity: i64) -> serde_json::Value {
    serde_json::json!({"node": {"quantity": quantity, "zone": zone, "card": {"name": name}}})
}

#[tokio::test]
async fn follows_manavault_deck_pages_and_pins_the_resolved_address() {
    let server = MockServer::start().await;
    let authority = format!("friend.home:{}", server.address().port());
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .and(header("host", authority.as_str()))
        .and(body_partial_json(
            serde_json::json!({"variables": {"id": "PagedPagedPagedPagedPage", "after": null}}),
        ))
        .respond_with(json(deck_page(
            &[node("commander", "Shorikai, Genesis Engine", 1)],
            Some("c1"),
        )))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/share/graphql"))
        .and(body_partial_json(
            serde_json::json!({"variables": {"after": "c1"}}),
        ))
        .respond_with(json(deck_page(
            &[
                node("mainboard", "Sol Ring", 1),
                node("mainboard", "Island", 30),
            ],
            None,
        )))
        .mount(&server)
        .await;
    let client = builder(&server).build().unwrap();

    let link = DeckLink::parse(&format!(
        "http://{authority}/share/decks/PagedPagedPagedPagedPage"
    ))
    .unwrap();
    let deck = client.fetch(&link).await.unwrap();
    assert_eq!(
        deck.url,
        format!("http://{authority}/share/decks/PagedPagedPagedPagedPage")
    );
    assert_eq!(deck.name.as_deref(), Some("Paged"));
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
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), 2);
    assert!(
        requests
            .iter()
            .all(|request: &Request| request.url.path() == "/share/graphql")
    );
}

#[tokio::test]
async fn manavault_lists_and_not_found() {
    let server = MockServer::start().await;
    Mock::given(path("/share/graphql"))
        .and(body_partial_json(
            serde_json::json!({"variables": {"id": "wants-tok"}}),
        ))
        .respond_with(json(serde_json::json!({"data": {"wantsList": {"entries": [
            {"cardName": "Sol Ring", "quantity": 2, "setCode": "c21", "collectorNumber": "263"}
        ]}}})))
        .mount(&server)
        .await;
    Mock::given(path("/share/graphql"))
        .and(body_partial_json(serde_json::json!({"variables": {"id": "binder-old"}})))
        .respond_with(json(serde_json::json!({"errors": [{"message": "Cannot query field \"binderList\" on type \"RootQueryType\"."}]})))
        .mount(&server)
        .await;
    Mock::given(path("/share/graphql"))
        .and(body_partial_json(
            serde_json::json!({"variables": {"id": "gone"}}),
        ))
        .respond_with(json(serde_json::json!({"data": {"deck": null}})))
        .mount(&server)
        .await;
    Mock::given(path("/share/graphql"))
        .and(body_partial_json(
            serde_json::json!({"variables": {"id": "errors"}}),
        ))
        .respond_with(json(
            serde_json::json!({"data": {"deck": null}, "errors": [{"message": "boom"}]}),
        ))
        .mount(&server)
        .await;
    let client = builder(&server).build().unwrap();
    let base = server.uri();

    let wants = client
        .fetch(&DeckLink::parse(&format!("{base}/share/wants/wants-tok")).unwrap())
        .await
        .unwrap();
    assert_eq!(wants.name.as_deref(), Some("Shared wants"));
    assert_eq!(wants.entries.len(), 1);
    assert_eq!(wants.entries[0].zone, Zone::Mainboard);
    assert_eq!(wants.entries[0].set_code.as_deref(), Some("c21"));

    assert_eq!(
        client
            .fetch(&DeckLink::parse(&format!("{base}/share/binder/binder-old")).unwrap())
            .await,
        Err(FetchError::Unsupported(ShareKind::Binder))
    );
    assert_eq!(
        client
            .fetch(&DeckLink::parse(&format!("{base}/share/decks/gone")).unwrap())
            .await,
        Err(FetchError::NotFound)
    );
    assert_eq!(
        client
            .fetch(&DeckLink::parse(&format!("{base}/share/decks/errors")).unwrap())
            .await,
        Err(FetchError::GraphqlErrors(vec!["boom".into()]))
    );
}

#[tokio::test]
async fn manavault_import_limits() {
    let server = MockServer::start().await;
    Mock::given(path("/share/graphql"))
        .and(body_partial_json(
            serde_json::json!({"variables": {"id": "endless"}}),
        ))
        .respond_with(json(deck_page(&[], Some("c1"))))
        .mount(&server)
        .await;
    Mock::given(path("/share/graphql"))
        .and(body_partial_json(
            serde_json::json!({"variables": {"id": "slow"}}),
        ))
        .respond_with(json(deck_page(&[], None)).set_delay(Duration::from_secs(2)))
        .mount(&server)
        .await;
    Mock::given(path("/share/graphql"))
        .and(body_partial_json(
            serde_json::json!({"variables": {"id": "huge"}}),
        ))
        .respond_with(json(deck_page(
            &(0..50)
                .map(|i| node("mainboard", &format!("Card {i}"), 1))
                .collect::<Vec<_>>(),
            None,
        )))
        .mount(&server)
        .await;
    let base = server.uri();

    let client = builder(&server)
        .limits(Limits {
            timeout: Duration::from_millis(300),
            ..Limits::default()
        })
        .build()
        .unwrap();
    assert_eq!(
        client
            .fetch(&DeckLink::parse(&format!("{base}/share/decks/endless")).unwrap())
            .await,
        Err(FetchError::InvalidPagination),
        "a cursor that repeats is rejected before the page cap"
    );
    assert_eq!(
        client
            .fetch(&DeckLink::parse(&format!("{base}/share/decks/slow")).unwrap())
            .await,
        Err(FetchError::LimitExceeded),
        "the whole-import deadline"
    );

    let client = builder(&server)
        .limits(Limits {
            max_page_bytes: 500,
            ..Limits::default()
        })
        .build()
        .unwrap();
    assert_eq!(
        client
            .fetch(&DeckLink::parse(&format!("{base}/share/decks/huge")).unwrap())
            .await,
        Err(FetchError::BodyTooLarge),
        "the per-page byte cap"
    );
}

#[tokio::test]
async fn destination_policy_blocks_before_any_request() {
    let server = MockServer::start().await;
    let base = server.uri();

    let strict = DecklistClient::builder("lotus tests")
        .resolver(Arc::new(FixedResolver(vec![loopback()])))
        .build()
        .unwrap();
    assert_eq!(
        strict
            .fetch(&DeckLink::parse(&format!("{base}/share/decks/tok")).unwrap())
            .await,
        Err(FetchError::BlockedDestination),
        "a loopback literal is not public"
    );

    let rebinding = DecklistClient::builder("lotus tests")
        .resolver(Arc::new(FixedResolver(vec![
            "93.184.216.34".parse().unwrap(),
            "169.254.169.254".parse().unwrap(),
        ])))
        .build()
        .unwrap();
    assert_eq!(
        rebinding
            .fetch(&DeckLink::parse("https://rebinding.example/share/decks/tok").unwrap())
            .await,
        Err(FetchError::BlockedDestination),
        "one private answer blocks the whole set"
    );

    let unresolvable = DecklistClient::builder("lotus tests")
        .resolver(Arc::new(FixedResolver(vec![])))
        .build()
        .unwrap();
    assert_eq!(
        unresolvable
            .fetch(&DeckLink::parse("https://nowhere.example/share/decks/tok").unwrap())
            .await,
        Err(FetchError::BlockedDestination)
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}
