//! Fetching deck lists over HTTP (feature `http`).
//!
//! The hardening both apps apply, in one place: no redirects, no cookies or
//! auth, bounded connect and receive time, a streamed body cap that does
//! not trust `Content-Length`, and for ManaVault links a destination policy
//! that resolves the host once, checks every answer, and pins the chosen
//! address while keeping the original hostname for the `Host` header and
//! TLS.

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::{Client, StatusCode, redirect};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::decklist::archidekt::{self, ArchidektDeck};
use crate::decklist::destination::{Allowlist, Origin};
use crate::decklist::link::{DeckLink, ShareKind, ShareLink};
use crate::decklist::manavault::{
    self, BinderData, DeckPager, GraphqlResponse, Limits, Step, WantsData,
};
use crate::decklist::moxfield::{self, MoxfieldDeck};
use crate::decklist::{Decklist, FetchError};

/// Response size cap for Moxfield and Archidekt (ManaVault's default).
pub const DEFAULT_MAX_BYTES: u64 = 5_000_000;
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolves a hostname to addresses. Implement this to stub DNS in tests or
/// to use a custom resolver.
pub trait Resolver: Send + Sync {
    /// Every address `host` resolves to.
    fn resolve<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send + 'a>>;
}

/// The operating system's resolver.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemResolver;

impl Resolver for SystemResolver {
    fn resolve<'a>(
        &'a self,
        host: &'a str,
    ) -> Pin<Box<dyn Future<Output = std::io::Result<Vec<IpAddr>>> + Send + 'a>> {
        Box::pin(async move {
            let mut addresses: Vec<IpAddr> = tokio::net::lookup_host((host, 0))
                .await?
                .map(|address| address.ip())
                .collect();
            addresses.dedup();
            Ok(addresses)
        })
    }
}

/// Configures a [`DecklistClient`].
#[derive(Clone)]
pub struct DecklistClientBuilder {
    user_agent: String,
    connect_timeout: Duration,
    timeout: Duration,
    max_bytes: u64,
    limits: Limits,
    allowlist: Allowlist,
    resolver: Arc<dyn Resolver>,
    moxfield_api_base: String,
    archidekt_api_base: String,
}

impl std::fmt::Debug for DecklistClientBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecklistClientBuilder")
            .field("user_agent", &self.user_agent)
            .field("connect_timeout", &self.connect_timeout)
            .field("timeout", &self.timeout)
            .field("max_bytes", &self.max_bytes)
            .field("limits", &self.limits)
            .field("allowlist", &self.allowlist)
            .field("moxfield_api_base", &self.moxfield_api_base)
            .field("archidekt_api_base", &self.archidekt_api_base)
            .finish_non_exhaustive()
    }
}

impl DecklistClientBuilder {
    /// Connect timeout for each request (default 10 s).
    #[must_use]
    pub fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Total timeout for each request (default 10 s).
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Body cap for Moxfield and Archidekt responses (default 5 MB).
    #[must_use]
    pub fn max_bytes(mut self, max_bytes: u64) -> Self {
        self.max_bytes = max_bytes;
        self
    }

    /// Budget for ManaVault imports.
    #[must_use]
    pub fn limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Non-public ManaVault destinations that may still be fetched.
    #[must_use]
    pub fn allowlist(mut self, allowlist: Allowlist) -> Self {
        self.allowlist = allowlist;
        self
    }

    /// DNS resolver for ManaVault hosts (default: the operating system's).
    #[must_use]
    pub fn resolver(mut self, resolver: Arc<dyn Resolver>) -> Self {
        self.resolver = resolver;
        self
    }

    /// Where to send Moxfield API requests, for tests and proxies.
    #[must_use]
    pub fn moxfield_api_base(mut self, base: impl Into<String>) -> Self {
        self.moxfield_api_base = base.into();
        self
    }

    /// Where to send Archidekt API requests, for tests and proxies.
    #[must_use]
    pub fn archidekt_api_base(mut self, base: impl Into<String>) -> Self {
        self.archidekt_api_base = base.into();
        self
    }

    /// Builds the client.
    pub fn build(self) -> Result<DecklistClient, FetchError> {
        let client = self
            .client_builder()
            .build()
            .map_err(|_| FetchError::RequestFailed)?;
        Ok(DecklistClient {
            client,
            config: self,
        })
    }

    fn client_builder(&self) -> reqwest::ClientBuilder {
        Client::builder()
            .user_agent(&self.user_agent)
            .redirect(redirect::Policy::none())
            .connect_timeout(self.connect_timeout)
            .timeout(self.timeout)
            .no_proxy()
    }
}

/// Fetches deck lists from Moxfield, Archidekt, and other ManaVault
/// instances.
#[derive(Debug, Clone)]
pub struct DecklistClient {
    client: Client,
    config: DecklistClientBuilder,
}

impl DecklistClient {
    /// Starts configuring a client that identifies as `user_agent`.
    #[must_use]
    pub fn builder(user_agent: &str) -> DecklistClientBuilder {
        DecklistClientBuilder {
            user_agent: user_agent.to_owned(),
            connect_timeout: DEFAULT_TIMEOUT,
            timeout: DEFAULT_TIMEOUT,
            max_bytes: DEFAULT_MAX_BYTES,
            limits: Limits::default(),
            allowlist: Allowlist::new(),
            resolver: Arc::new(SystemResolver),
            moxfield_api_base: moxfield::API_BASE.to_owned(),
            archidekt_api_base: archidekt::API_BASE.to_owned(),
        }
    }

    /// A client with every default.
    pub fn new(user_agent: &str) -> Result<Self, FetchError> {
        Self::builder(user_agent).build()
    }

    /// The configured ManaVault import limits.
    #[must_use]
    pub fn limits(&self) -> Limits {
        self.config.limits
    }

    /// Fetches the list a link points at. A host-less ManaVault path and
    /// an unknown site are [`FetchError::UnsupportedLink`]: the app
    /// resolves local shares itself.
    pub async fn fetch(&self, link: &DeckLink) -> Result<Decklist, FetchError> {
        match link {
            DeckLink::Moxfield { id } => self.fetch_moxfield(id).await,
            DeckLink::Archidekt { id } => self.fetch_archidekt(id).await,
            DeckLink::ManaVault {
                origin: Some(origin),
                share,
            } => self.fetch_manavault(origin, share).await,
            DeckLink::ManaVault { origin: None, .. } | DeckLink::Other { .. } => {
                Err(FetchError::UnsupportedLink)
            }
        }
    }

    /// Fetches a Moxfield deck by validated id.
    pub async fn fetch_moxfield(&self, id: &str) -> Result<Decklist, FetchError> {
        let url = moxfield::api_url(&self.config.moxfield_api_base, id);
        let (deck, _bytes): (MoxfieldDeck, u64) =
            get_json(&self.client, &url, self.config.max_bytes).await?;
        Ok(deck.into_decklist(id))
    }

    /// Fetches an Archidekt deck by validated id.
    pub async fn fetch_archidekt(&self, id: &str) -> Result<Decklist, FetchError> {
        let url = archidekt::api_url(&self.config.archidekt_api_base, id);
        let (deck, _bytes): (ArchidektDeck, u64) =
            get_json(&self.client, &url, self.config.max_bytes).await?;
        Ok(deck.into_decklist(id))
    }

    /// Fetches a shared deck, want list, or binder from another ManaVault
    /// instance. The whole import, including DNS, must finish within
    /// [`Limits::timeout`], else [`FetchError::LimitExceeded`].
    pub async fn fetch_manavault(
        &self,
        origin: &Origin,
        share: &ShareLink,
    ) -> Result<Decklist, FetchError> {
        let limits = self.config.limits;
        tokio::time::timeout(
            limits.timeout,
            self.fetch_manavault_unbounded(origin, share),
        )
        .await
        .unwrap_or(Err(FetchError::LimitExceeded))
    }

    async fn fetch_manavault_unbounded(
        &self,
        origin: &Origin,
        share: &ShareLink,
    ) -> Result<Decklist, FetchError> {
        let client = self.pinned_client(origin).await?;
        let endpoint = origin.join(manavault::GRAPHQL_PATH);
        let canonical_url = origin.join(&share.path());
        let limits = self.config.limits;
        match share.kind {
            ShareKind::Deck => {
                let mut pager = DeckPager::new(share.token.clone(), limits);
                loop {
                    let allowance = pager.page_allowance()?;
                    let (response, bytes): (GraphqlResponse<manavault::DeckData>, u64) =
                        post_json(&client, &endpoint, &pager.request(), allowance).await?;
                    if pager.accept(response, bytes)? == Step::Done {
                        return Ok(pager.finish(canonical_url));
                    }
                }
            }
            ShareKind::Wants => {
                let request = manavault::list_request(share.kind, &share.token)
                    .ok_or(FetchError::Malformed)?;
                let allowance = manavault::Budget::new(limits).page_allowance()?;
                let (response, _bytes): (GraphqlResponse<WantsData>, u64) =
                    post_json(&client, &endpoint, &request, allowance).await?;
                let data = response
                    .into_data()
                    .map_err(|error| manavault::classify_list_error(share.kind, error))?;
                manavault::list_decklist(share, canonical_url, data.wants_list, limits)
            }
            ShareKind::Binder => {
                let request = manavault::list_request(share.kind, &share.token)
                    .ok_or(FetchError::Malformed)?;
                let allowance = manavault::Budget::new(limits).page_allowance()?;
                let (response, _bytes): (GraphqlResponse<BinderData>, u64) =
                    post_json(&client, &endpoint, &request, allowance).await?;
                let data = response
                    .into_data()
                    .map_err(|error| manavault::classify_list_error(share.kind, error))?;
                manavault::list_decklist(share, canonical_url, data.binder_list, limits)
            }
        }
    }

    /// Resolves the origin's host, applies the destination policy to every
    /// answer, and returns a client pinned to the first address.
    async fn pinned_client(&self, origin: &Origin) -> Result<Client, FetchError> {
        let addresses = match origin.ip_literal() {
            Some(address) => vec![address],
            None => self
                .config
                .resolver
                .resolve(&origin.host)
                .await
                .map_err(|_| FetchError::BlockedDestination)?,
        };
        if !self.config.allowlist.allows(&origin.host, &addresses) {
            return Err(FetchError::BlockedDestination);
        }
        let address = addresses
            .first()
            .copied()
            .ok_or(FetchError::BlockedDestination)?;
        let mut builder = self.config.client_builder();
        if origin.ip_literal().is_none() {
            builder = builder.resolve(
                &origin.host,
                SocketAddr::new(address, origin.port_or_default()),
            );
        }
        builder.build().map_err(|_| FetchError::RequestFailed)
    }
}

async fn get_json<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    max_bytes: u64,
) -> Result<(T, u64), FetchError> {
    let request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json");
    send_json(request, max_bytes).await
}

async fn post_json<T: DeserializeOwned>(
    client: &Client,
    url: &str,
    body: &impl Serialize,
    max_bytes: u64,
) -> Result<(T, u64), FetchError> {
    let request = client
        .post(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .json(body);
    send_json(request, max_bytes).await
}

/// Sends a request and decodes its JSON body, mapping failures to
/// [`FetchError`]: 401 and 403 are [`FetchError::Forbidden`], 404 is
/// [`FetchError::NotFound`], other non-success statuses are
/// [`FetchError::HttpStatus`], a body over `max_bytes` is
/// [`FetchError::BodyTooLarge`], non-JSON is [`FetchError::InvalidJson`],
/// and JSON of the wrong shape is [`FetchError::Malformed`].
async fn send_json<T: DeserializeOwned>(
    request: reqwest::RequestBuilder,
    max_bytes: u64,
) -> Result<(T, u64), FetchError> {
    let response = request
        .send()
        .await
        .map_err(|error| transport_error(&error))?;
    let status = response.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Err(FetchError::Forbidden);
    }
    if status == StatusCode::NOT_FOUND {
        return Err(FetchError::NotFound);
    }
    if !status.is_success() {
        return Err(FetchError::HttpStatus(status.as_u16()));
    }
    let body = read_capped(response, max_bytes).await?;
    let bytes = u64::try_from(body.len()).unwrap_or(u64::MAX);
    let value: serde_json::Value =
        serde_json::from_slice(&body).map_err(|_| FetchError::InvalidJson)?;
    // serde would happily read a JSON array into a struct positionally;
    // every source answers with an object.
    if !value.is_object() {
        return Err(FetchError::Malformed);
    }
    let decoded = serde_json::from_value(value).map_err(|_| FetchError::Malformed)?;
    Ok((decoded, bytes))
}

/// Streams a body, stopping as soon as it exceeds `max_bytes`.
async fn read_capped(response: reqwest::Response, max_bytes: u64) -> Result<Vec<u8>, FetchError> {
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|error| transport_error(&error))?;
        let received = u64::try_from(body.len() + chunk.len()).unwrap_or(u64::MAX);
        if received > max_bytes {
            return Err(FetchError::BodyTooLarge);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

fn transport_error(error: &reqwest::Error) -> FetchError {
    if error.is_timeout() {
        FetchError::Timeout
    } else {
        FetchError::RequestFailed
    }
}
