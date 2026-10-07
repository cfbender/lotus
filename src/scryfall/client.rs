//! A small Scryfall API client (feature `http`).
//!
//! Scryfall asks clients to identify themselves with a `User-Agent` and to
//! send `Accept: application/json`; the client sets both. Rate limiting is
//! left to the caller: both apps already own a limiter.

use std::path::Path;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Client, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::io::AsyncWriteExt;

use crate::ids::{OracleId, ScryfallId};
use crate::scryfall::bulk::{BULK_DATA_URL, BulkData, BulkDataList};
use crate::scryfall::card::ScryfallCard;
use crate::scryfall::rulings::{Ruling, RulingsList};

const API_BASE: &str = "https://api.scryfall.com";

/// Why a Scryfall request failed.
#[derive(Debug, thiserror::Error)]
pub enum ScryfallError {
    /// Scryfall returned 404.
    #[error("Scryfall has no such resource")]
    NotFound,
    /// Scryfall returned a non-success status.
    #[error("Scryfall request failed with HTTP {0}")]
    Status(StatusCode),
    /// The request could not be completed.
    #[error("Scryfall request failed: {0}")]
    Transport(#[from] reqwest::Error),
    /// The response body was not the expected JSON.
    #[error("Scryfall returned an unreadable response: {0}")]
    Decode(#[from] serde_json::Error),
    /// Writing a download to disk failed.
    #[error("could not write Scryfall download: {0}")]
    Io(#[from] std::io::Error),
}

/// One page of `GET /cards/search`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SearchPage {
    /// The cards on this page.
    #[serde(default)]
    pub data: Vec<ScryfallCard>,
    /// Whether another page follows.
    #[serde(default)]
    pub has_more: bool,
    /// Total matching cards.
    #[serde(default)]
    pub total_cards: Option<u64>,
}

/// A Scryfall API client.
#[derive(Debug, Clone)]
pub struct ScryfallClient {
    client: Client,
    api_base: String,
}

impl ScryfallClient {
    /// Builds a client that identifies as `user_agent`, with a 3 s connect
    /// timeout and a 10 s request timeout for API calls.
    pub fn new(user_agent: &str) -> Result<Self, ScryfallError> {
        let client = Client::builder()
            .user_agent(user_agent)
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self {
            client,
            api_base: API_BASE.to_owned(),
        })
    }

    /// Builds a client from a preconfigured `reqwest::Client`, with API
    /// requests sent to `api_base` instead of `https://api.scryfall.com`.
    /// Intended for tests and proxies.
    #[must_use]
    pub fn with_client(client: Client, api_base: impl Into<String>) -> Self {
        Self {
            client,
            api_base: api_base.into(),
        }
    }

    /// The configured API base URL.
    #[must_use]
    pub fn api_base(&self) -> &str {
        &self.api_base
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.api_base, path)
    }

    /// `GET`s `url` and decodes a JSON body. A 404 is [`ScryfallError::NotFound`].
    pub async fn get_json<T: DeserializeOwned>(&self, url: &str) -> Result<T, ScryfallError> {
        let response = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await?;
        let status = response.status();
        if status == StatusCode::NOT_FOUND {
            return Err(ScryfallError::NotFound);
        }
        if !status.is_success() {
            return Err(ScryfallError::Status(status));
        }
        let body = response.bytes().await?;
        Ok(serde_json::from_slice(&body)?)
    }

    /// `GET /bulk-data`.
    pub async fn bulk_data_list(&self) -> Result<BulkDataList, ScryfallError> {
        let url = if self.api_base == API_BASE {
            BULK_DATA_URL.to_owned()
        } else {
            self.url("/bulk-data")
        };
        self.get_json(&url).await
    }

    /// `GET /bulk-data/:type`, such as `default-cards` or `oracle-cards`.
    pub async fn bulk_data(&self, kind: &str) -> Result<BulkData, ScryfallError> {
        self.get_json(&self.url(&format!("/bulk-data/{kind}")))
            .await
    }

    /// Downloads `uri` into memory. Bulk files are gigabytes uncompressed and
    /// hundreds of megabytes compressed; prefer [`Self::download_to_file`]
    /// unless the payload is known to be small.
    pub async fn download_bytes(&self, uri: &str) -> Result<Vec<u8>, ScryfallError> {
        let response = self
            .client
            .get(uri)
            .timeout(Duration::from_mins(30))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ScryfallError::Status(status));
        }
        Ok(response.bytes().await?.to_vec())
    }

    /// Streams `uri` to `path`, returning the number of bytes written. The
    /// request may take up to 30 minutes. A failed download removes the
    /// partial file.
    pub async fn download_to_file(&self, uri: &str, path: &Path) -> Result<u64, ScryfallError> {
        let response = self
            .client
            .get(uri)
            .timeout(Duration::from_mins(30))
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            return Err(ScryfallError::Status(status));
        }
        let result = write_stream(response, path).await;
        if result.is_err() {
            let _ = tokio::fs::remove_file(path).await;
        }
        result
    }

    /// `GET /cards/:id`.
    pub async fn card(&self, id: &ScryfallId) -> Result<ScryfallCard, ScryfallError> {
        self.get_json(&self.url(&format!("/cards/{}", encode(id.as_str()))))
            .await
    }

    /// `GET /cards/:id/rulings`.
    pub async fn rulings(&self, id: &ScryfallId) -> Result<Vec<Ruling>, ScryfallError> {
        let list: RulingsList = self
            .get_json(&self.url(&format!("/cards/{}/rulings", encode(id.as_str()))))
            .await?;
        Ok(list.data)
    }

    /// `GET`s a card's `rulings_uri` as served in the bulk file.
    pub async fn rulings_at(&self, rulings_uri: &str) -> Result<Vec<Ruling>, ScryfallError> {
        let list: RulingsList = self.get_json(rulings_uri).await?;
        Ok(list.data)
    }

    /// One page of every English paper printing of a card, oldest first,
    /// including variations. A 404 (Scryfall's answer for no matches) is an
    /// empty page.
    pub async fn printings(
        &self,
        oracle_id: &OracleId,
        page: u32,
    ) -> Result<SearchPage, ScryfallError> {
        let query = format!("oracleid:{} game:paper lang:en", oracle_id.as_str());
        let url = format!(
            "{}/cards/search?q={}&unique=prints&order=released&include_variations=true&page={page}",
            self.api_base,
            encode(&query)
        );
        match self.get_json(&url).await {
            Ok(page) => Ok(page),
            Err(ScryfallError::NotFound) => Ok(SearchPage {
                data: Vec::new(),
                has_more: false,
                total_cards: Some(0),
            }),
            Err(error) => Err(error),
        }
    }
}

async fn write_stream(response: reqwest::Response, path: &Path) -> Result<u64, ScryfallError> {
    let mut file = tokio::fs::File::create(path).await?;
    let mut stream = response.bytes_stream();
    let mut written = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk).await?;
        written += u64::try_from(chunk.len()).unwrap_or(u64::MAX);
    }
    file.flush().await?;
    Ok(written)
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}
