//! The `sqlx` feature: domain types round-trip through SQLite columns using
//! the text and integer encodings the Elixir apps already store.

#![cfg(feature = "sqlx")]

use lotus::card::{Condition, Finish, Zone};
use lotus::commander::CommanderPairing;
use lotus::decklist::Source;
use lotus::{OracleId, Quantity, ScryfallId};
use sqlx::SqlitePool;

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;

async fn pool() -> TestResult<SqlitePool> {
    let pool = SqlitePool::connect("sqlite::memory:").await?;
    sqlx::query(
        "CREATE TABLE rows (
            oracle_id TEXT NOT NULL,
            scryfall_id TEXT NOT NULL,
            quantity INTEGER NOT NULL,
            finish TEXT NOT NULL,
            condition TEXT NOT NULL,
            zone TEXT NOT NULL,
            source TEXT NOT NULL,
            pairing TEXT
        )",
    )
    .execute(&pool)
    .await?;
    Ok(pool)
}

async fn stored<T>(pool: &SqlitePool, value: T) -> TestResult<String>
where
    T: for<'q> sqlx::Encode<'q, sqlx::Sqlite> + sqlx::Type<sqlx::Sqlite> + Send,
{
    let (text,): (String,) = sqlx::query_as("SELECT ?")
        .bind(value)
        .fetch_one(pool)
        .await?;
    Ok(text)
}

#[derive(Debug, PartialEq, Eq, sqlx::FromRow)]
struct Row {
    oracle_id: OracleId,
    scryfall_id: ScryfallId,
    quantity: Quantity,
    finish: Finish,
    condition: Condition,
    zone: Zone,
    source: Source,
    pairing: Option<CommanderPairing>,
}

#[tokio::test]
async fn round_trips_typed_columns() {
    let pool = pool().await.unwrap();
    let row = Row {
        oracle_id: OracleId::new("o-1"),
        scryfall_id: ScryfallId::new("s-1"),
        quantity: Quantity::new(4).unwrap(),
        finish: Finish::Etched,
        condition: Condition::LightlyPlayed,
        zone: Zone::Considering,
        source: Source::ManaVault,
        pairing: Some(CommanderPairing::FriendsForever),
    };
    sqlx::query("INSERT INTO rows VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
        .bind(&row.oracle_id)
        .bind(&row.scryfall_id)
        .bind(row.quantity)
        .bind(row.finish)
        .bind(row.condition)
        .bind(row.zone)
        .bind(row.source)
        .bind(row.pairing)
        .execute(&pool)
        .await
        .unwrap();

    let stored: (String, String, i64, String, String, String, String, String) =
        sqlx::query_as("SELECT * FROM rows")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        stored,
        (
            "o-1".into(),
            "s-1".into(),
            4,
            "etched".into(),
            "lightly_played".into(),
            "considering".into(),
            "manavault".into(),
            "friends_forever".into()
        ),
        "the stored encodings match the Elixir apps' columns"
    );

    let read: Row = sqlx::query_as("SELECT * FROM rows")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(read, row);
}

#[tokio::test]
async fn rejects_invalid_stored_values() {
    let pool = pool().await.unwrap();
    sqlx::query(
        "INSERT INTO rows VALUES ('o', 's', 0, 'foil', 'near_mint', 'mainboard', 'moxfield', NULL)",
    )
    .execute(&pool)
    .await
    .unwrap();
    let zero: Result<(Quantity,), _> = sqlx::query_as("SELECT quantity FROM rows")
        .fetch_one(&pool)
        .await;
    assert!(zero.is_err(), "a zero quantity does not decode");

    sqlx::query("UPDATE rows SET quantity = 1, finish = 'glossy'")
        .execute(&pool)
        .await
        .unwrap();
    let finish: Result<(Finish,), _> = sqlx::query_as("SELECT finish FROM rows")
        .fetch_one(&pool)
        .await;
    assert!(finish.is_err(), "an unknown finish does not decode");
    let quantity: (Quantity,) = sqlx::query_as("SELECT quantity FROM rows")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(quantity.0, Quantity::ONE);
}

/// Every enum's stored text is its `as_str`, for each variant, so a Rust
/// backend reads and writes the columns the Elixir apps populate.
#[tokio::test]
async fn enum_encodings_match_as_str() {
    let pool = pool().await.unwrap();
    for finish in [Finish::Nonfoil, Finish::Foil, Finish::Etched] {
        assert_eq!(stored(&pool, finish).await.unwrap(), finish.as_str());
    }
    for condition in [
        Condition::NearMint,
        Condition::LightlyPlayed,
        Condition::ModeratelyPlayed,
        Condition::HeavilyPlayed,
        Condition::Damaged,
    ] {
        assert_eq!(stored(&pool, condition).await.unwrap(), condition.as_str());
    }
    for zone in [Zone::Mainboard, Zone::Commander, Zone::Considering] {
        assert_eq!(stored(&pool, zone).await.unwrap(), zone.as_str());
    }
    for source in [Source::Moxfield, Source::Archidekt, Source::ManaVault] {
        assert_eq!(stored(&pool, source).await.unwrap(), source.as_str());
        assert_eq!(serde_json::to_value(source).unwrap(), source.as_str());
    }
    for pairing in [
        CommanderPairing::Background,
        CommanderPairing::FriendsForever,
        CommanderPairing::ChooseABackground,
        CommanderPairing::PartnerWith,
        CommanderPairing::DoctorsCompanion,
        CommanderPairing::Partner,
        CommanderPairing::Doctor,
    ] {
        assert_eq!(stored(&pool, pairing).await.unwrap(), pairing.as_str());
        assert_eq!(serde_json::to_value(pairing).unwrap(), pairing.as_str());
    }
}
