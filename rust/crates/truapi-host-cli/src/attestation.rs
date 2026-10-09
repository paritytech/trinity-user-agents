//! Lite-username attestation against the identity backend.
//!
//! Fetches the backend verifier. Builds the client proofs
//! (`truapi::host_logic::attestation`), including the dotNS gateway
//! reservation signature timestamped with Asset Hub chain time. POSTs them to
//! `/usernames`. Polls the dotNS contracts on Asset Hub until the lite username
//! lands.
//!
//! Registers the signing host's RFC-0022 `uid.<suffix>` identity account. The paired
//! host can then resolve its username via `get_user_id`.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use futures_util::{StreamExt as _, TryStreamExt as _, stream};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};
use std::collections::BTreeSet;
use std::sync::Mutex;
use tracing::{debug, warn};
use truapi::host_logic::attestation::build_lite_registration;
use truapi::host_logic::dotns_gateway::{
    MAX_BASE_LABEL_LEN, MIN_PERSON_LABEL_LEN, is_registrable_full_label,
};
use truapi::host_logic::product_account::{
    SR25519_SIGNING_CONTEXT, derive_identity_keypair, derive_root_keypair_from_entropy,
    identity_product_id, product_public_key_to_address,
};

use crate::dotns_read::AssetHubReader;
use crate::network::NetworkConfig;

/// Env var carrying an optional bearer token for the identity backend.
/// Set it to reuse a token minted elsewhere. Unset, the CLI runs the sr25519
/// auth handshake itself ([`backend_token`]).
pub const IDENTITY_BACKEND_TOKEN_ENV: &str = "HOST_CLI_IDENTITY_BACKEND_TOKEN";

/// Access tokens for the username routes, keyed by backend and authenticated
/// account. The backend requires a username registration's candidate account
/// to be the JWT subject, so a token must never cross candidate identities.
static BACKEND_TOKENS: Mutex<Vec<CachedBackendToken>> = Mutex::new(Vec::new());

#[derive(Clone, Debug, PartialEq, Eq)]
struct CachedBackendToken {
    backend_base: String,
    auth_client_id: [u8; 32],
    value: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BackendTokenSource {
    Environment,
    Cache,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BackendToken {
    value: String,
    source: BackendTokenSource,
    auth_client_id: [u8; 32],
}

/// Bearer token for the identity backend's username routes.
///
/// The explicit env token wins. Otherwise the CLI completes the backend's
/// `challenges` → `token` sr25519 handshake as the mnemonic's RFC-0022
/// `uid.<suffix>` account on the network being attested. Since
/// device-uniqueness-backend#77, `POST /usernames` rejects a JWT whose subject
/// differs from `candidateAccountId`.
async fn backend_token(
    client: &reqwest::Client,
    backend_base: &str,
    auth_entropy: &[u8],
    network_suffix: &str,
) -> Result<BackendToken> {
    let auth_client_id = derive_identity_keypair(auth_entropy, network_suffix)
        .map_err(|err| anyhow::anyhow!("backend auth identity derivation failed: {err}"))?
        .public
        .to_bytes();
    if let Ok(token) = std::env::var(IDENTITY_BACKEND_TOKEN_ENV)
        && !token.trim().is_empty()
    {
        return Ok(BackendToken {
            value: token.trim().to_string(),
            source: BackendTokenSource::Environment,
            auth_client_id,
        });
    }
    let cached = BACKEND_TOKENS
        .lock()
        .expect("backend token cache mutex poisoned")
        .iter()
        .find(|token| token.backend_base == backend_base && token.auth_client_id == auth_client_id)
        .map(|token| token.value.clone());
    if let Some(token) = cached {
        return Ok(BackendToken {
            value: token,
            source: BackendTokenSource::Cache,
            auth_client_id,
        });
    }
    let token = mint_backend_token(client, backend_base, auth_entropy, network_suffix).await?;
    let token = {
        let mut tokens = BACKEND_TOKENS
            .lock()
            .expect("backend token cache mutex poisoned");
        if let Some(existing) = tokens.iter().find(|token| {
            token.backend_base == backend_base && token.auth_client_id == auth_client_id
        }) {
            existing.value.clone()
        } else {
            tokens.push(CachedBackendToken {
                backend_base: backend_base.to_string(),
                auth_client_id,
                value: token.clone(),
            });
            token
        }
    };
    Ok(BackendToken {
        value: token,
        source: BackendTokenSource::Cache,
        auth_client_id,
    })
}

fn evict_rejected_backend_token(backend_base: &str, auth_client_id: &[u8; 32], rejected: &str) {
    BACKEND_TOKENS
        .lock()
        .expect("backend token cache mutex poisoned")
        .retain(|token| {
            token.backend_base != backend_base
                || token.auth_client_id != *auth_client_id
                || token.value != rejected
        });
}

/// Runs the backend's auth handshake and returns its access JWT.
async fn mint_backend_token(
    client: &reqwest::Client,
    backend_base: &str,
    auth_entropy: &[u8],
    network_suffix: &str,
) -> Result<String> {
    let url = format!("{backend_base}/auth/challenges");
    let body: Value = client
        .post(&url)
        .json(&json!({}))
        .send()
        .await
        .with_context(|| format!("POST {url}"))?
        .error_for_status()?
        .json()
        .await
        .context("decoding challenge response")?;
    let challenge = body
        .get("challenge")
        .and_then(Value::as_str)
        .context("challenge response missing 'challenge' field")?
        .to_string();
    let challenge_bytes = BASE64
        .decode(&challenge)
        .context("challenge is not valid base64")?;

    let keypair = derive_identity_keypair(auth_entropy, network_suffix)
        .map_err(|err| anyhow::anyhow!("backend auth identity derivation failed: {err}"))?;
    let client_id = keypair.public.to_bytes();

    // The proof signs SHA256(challenge || clientId || SHA256(body)). It must
    // cover the exact bytes the request carries, so the body is serialized once.
    let payload = b"{}";
    let mut hasher = Sha256::new();
    hasher.update(&challenge_bytes);
    hasher.update(client_id);
    hasher.update(Sha256::digest(payload));
    let message: [u8; 32] = hasher.finalize().into();
    let proof = keypair
        .secret
        .sign_simple(SR25519_SIGNING_CONTEXT, &message, &keypair.public)
        .to_bytes();

    let url = format!("{backend_base}/auth/token");
    let response = client
        .post(&url)
        .header("Auth-ClientId", BASE64.encode(client_id))
        .header("Auth-ClientProof", BASE64.encode(proof))
        .header("Auth-Challenge", &challenge)
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(payload.as_slice())
        .send()
        .await
        .with_context(|| format!("POST {url}"))?;
    let status = response.status();
    let body = response.text().await.unwrap_or_default();
    if !status.is_success() {
        bail!("identity backend auth handshake failed ({status}): {body}");
    }
    let body: Value = serde_json::from_str(&body).context("decoding token response")?;
    let token = body
        .get("token")
        .and_then(Value::as_str)
        .context("token response missing 'token' field")?;
    debug!("minted identity backend access token");
    Ok(token.to_string())
}

/// Send one authenticated backend request. A cached token can expire while an
/// interactive host stays open, so a 401 evicts it, re-runs the authentication
/// handshake, and retries the request once. An explicit environment token is
/// never silently replaced.
async fn send_with_backend_auth<F>(
    client: &reqwest::Client,
    backend_base: &str,
    auth_entropy: &[u8],
    network_suffix: &str,
    request: F,
) -> Result<reqwest::Response>
where
    F: Fn(&str) -> reqwest::RequestBuilder,
{
    let token = backend_token(client, backend_base, auth_entropy, network_suffix).await?;
    let response = request(&token.value).send().await?;
    if response.status() != reqwest::StatusCode::UNAUTHORIZED
        || token.source == BackendTokenSource::Environment
    {
        return Ok(response);
    }

    warn!(backend = %backend_base, "identity backend rejected cached token; authenticating again");
    evict_rejected_backend_token(backend_base, &token.auth_client_id, &token.value);
    let refreshed = backend_token(client, backend_base, auth_entropy, network_suffix)
        .await
        .context("refresh identity backend token after 401 Unauthorized")?;
    request(&refreshed.value).send().await.map_err(Into::into)
}

/// Inputs for one attestation run.
pub struct AttestConfig {
    /// Identity backend base URL including `/api/v1`.
    pub backend_base: String,
    /// Asset Hub WebSocket URL for the reservation timestamp and the dotNS
    /// username poll.
    pub asset_hub_ws: String,
    /// The network's dotNS TLD without the dot (`paseo`, `testnet`); the
    /// registered person is `uid.<suffix>` / `peopl.<suffix>`.
    pub network_suffix: String,
    /// BIP-39 entropy of the signing host's root account.
    pub entropy: Vec<u8>,
    /// Requested lite username base (6+ lowercase letters, no digits).
    pub username_base: String,
    /// Optional base name to queue on dotNS for a later full-person claim.
    pub reserved_username: Option<String>,
}

/// Check whether a lite username base is available through the identity
/// backend. The username must be the base form without the digit suffix.
pub async fn lite_username_available(
    network: NetworkConfig,
    auth_entropy: &[u8],
    username_base: &str,
) -> Result<bool> {
    let backend_base = network.identity_backend_base;
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let url = format!("{backend_base}/usernames/available");
    let body = json!({ "usernames": [username_base] });
    let response = send_with_backend_auth(
        &client,
        backend_base,
        auth_entropy,
        network.network_suffix,
        |token| client.post(&url).bearer_auth(token).json(&body),
    )
    .await
    .with_context(|| format!("POST {url}"))?;
    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        bail!("username availability check failed for {username_base} ({status}): {text}");
    }
    let body: Value = response
        .json()
        .await
        .context("decoding availability response")?;
    Ok(availability_status(&body, username_base) == Some("AVAILABLE"))
}

/// Reads one base's status out of the availability response, a flat
/// `{base: "AVAILABLE" | …}` map.
fn availability_status<'a>(body: &'a Value, username_base: &str) -> Option<&'a str> {
    body.get(username_base).and_then(Value::as_str).or_else(|| {
        body.get("value")?
            .get(username_base)?
            .get("status")?
            .as_str()
    })
}

/// Registers (or confirms) the signing host's lite username. Waits until the
/// dotNS contracts on Asset Hub record it. Returns the lite username assigned on
/// chain, including its discriminator.
pub async fn attest(config: &AttestConfig) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;

    let mut reader = AssetHubReader::connect(&config.asset_hub_ws).await?;
    // Timestamping the reservation signature with Asset Hub chain time. The
    // gateway rejects values ahead of the chain.
    let signed_at = reader
        .timestamp_secs()
        .await
        .context("read Asset Hub Timestamp.Now")?;

    // A reserved base name that is already registered can never be claimed,
    // yet the reservation would hold that stem's queue for the whole
    // reservation window; neither the backend nor the gateway checks it. Ask
    // the registrar first.
    if let Some(reserved) = config.reserved_username.as_deref() {
        if !is_registrable_full_label(reserved) {
            bail!(
                "reserved username {reserved:?} is not a reservable base label (lowercase ASCII \
                 letters only, {MIN_PERSON_LABEL_LEN} to {MAX_BASE_LABEL_LEN} bytes); the \
                 gateway would reject the whole attestation"
            );
        }
        if !reader.label_available(reserved).await? {
            bail!(
                "reserved username {reserved:?} is already registered on dotNS; a reservation \
                 for it could never be claimed and would lock the stem's reservation queue"
            );
        }
    }

    let verifier = fetch_verifier(&client, &config.backend_base).await?;
    let registration = build_lite_registration(
        &config.entropy,
        &config.network_suffix,
        verifier,
        &config.username_base,
        config.reserved_username.as_deref(),
        signed_at,
    )
    .map_err(|reason| anyhow::anyhow!("failed to build registration params: {reason}"))?;
    debug!(
        candidate = %registration.candidate_account_id,
        "attesting lite username '{}'",
        config.username_base
    );

    submit_registration(&client, config, signed_at, &registration).await?;

    let identity = wait_for_dotns_username(&mut reader, &registration.candidate_public_key).await?;
    debug!("lite username registered and confirmed on-chain");
    identity
        .lite_username
        .context("registered dotNS identity has no lite username")
}

/// Resolves the on-chain lite username for an already-attested signer: the
/// discriminated `name.NN` the dotNS contracts hold for its identity account,
/// whatever base the account record asked for.
pub async fn registered_lite_username(network: NetworkConfig, entropy: &[u8]) -> Result<String> {
    let identity = derive_identity_keypair(entropy, network.network_suffix)
        .map_err(|err| anyhow::anyhow!("uid identity derivation failed: {err}"))?;
    let mut reader = AssetHubReader::connect(network.asset_hub_ws).await?;
    reader
        .dotns_identity(&identity.public.to_bytes())
        .await?
        .lite_username
        .context("attested signer has no dotNS lite username")
}

/// Resolve an existing full-person or Lite dotNS identity when one exists.
/// An unlabeled account is a valid result and can still back a local session.
pub async fn lookup_registered_username(
    network: NetworkConfig,
    entropy: &[u8],
) -> Result<Option<String>> {
    let identity = derive_identity_keypair(entropy, network.network_suffix)
        .map_err(|err| anyhow::anyhow!("uid identity derivation failed: {err}"))?;
    let mut reader = AssetHubReader::connect(network.asset_hub_ws).await?;
    let identity = reader.dotns_identity(&identity.public.to_bytes()).await?;
    Ok(identity.full_username.or(identity.lite_username))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsernameSearchPage {
    usernames: Vec<UsernameSearchItem>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct UsernameSearchItem {
    account_id: String,
    username: String,
    status: String,
}

/// Reverse-resolve an assigned username from the identity backend.
///
/// The backend does not currently expose an account-indexed route. Its search
/// route is cursor-paginated and prefix-only, so imports search each valid
/// initial letter and retain only rows whose candidate account is the mnemonic's
/// `uid.<suffix>` identity on this network. The bearer token exempts these calls
/// from the unauthenticated proof-of-compute challenge.
pub async fn lookup_backend_username(
    network: NetworkConfig,
    auth_entropy: &[u8],
    candidate_account_id: &str,
) -> Result<Option<String>> {
    const SEARCH_CONCURRENCY: usize = 4;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let matches = stream::iter(b'a'..=b'z')
        .map(|initial| {
            search_backend_prefix(
                &client,
                network,
                auth_entropy,
                candidate_account_id,
                char::from(initial),
            )
        })
        .buffer_unordered(SEARCH_CONCURRENCY)
        .try_collect::<Vec<_>>()
        .await?;
    let matches = matches.into_iter().flatten().collect::<BTreeSet<_>>();

    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => {
            bail!(
                "identity backend returned multiple assigned usernames for {candidate_account_id}: {}",
                matches.into_iter().collect::<Vec<_>>().join(", ")
            )
        }
    }
}

async fn search_backend_prefix(
    client: &reqwest::Client,
    network: NetworkConfig,
    auth_entropy: &[u8],
    candidate_account_id: &str,
    initial: char,
) -> Result<Vec<String>> {
    const PAGE_LIMIT: &str = "1000";

    let backend_base = network.identity_backend_base;
    let url = format!("{backend_base}/usernames/search");
    let prefix = initial.to_string();
    let mut cursor = None;
    let mut seen_cursors = BTreeSet::new();
    let mut matches = Vec::new();
    loop {
        let mut query = vec![("prefix", prefix.as_str()), ("limit", PAGE_LIMIT)];
        if let Some(cursor) = cursor.as_deref() {
            query.push(("cursor", cursor));
        }
        let response = send_with_backend_auth(
            client,
            backend_base,
            auth_entropy,
            network.network_suffix,
            |token| client.get(&url).bearer_auth(token).query(&query),
        )
        .await
        .with_context(|| format!("GET {url} for prefix {prefix:?}"))?;
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        if !status.is_success() {
            bail!("identity backend username search failed ({status}): {text}");
        }
        let page: UsernameSearchPage = serde_json::from_str(&text)
            .with_context(|| format!("decode identity backend username search response: {text}"))?;
        matches.extend(matching_assigned_usernames(
            page.usernames,
            candidate_account_id,
        ));

        let Some(next) = page.next_cursor else {
            break;
        };
        if !seen_cursors.insert(next.clone()) {
            bail!("identity backend username search repeated cursor {next:?}");
        }
        cursor = Some(next);
    }
    Ok(matches)
}

fn matching_assigned_usernames(
    items: Vec<UsernameSearchItem>,
    candidate_account_id: &str,
) -> impl Iterator<Item = String> {
    let candidate_account_id = candidate_account_id.to_string();
    items
        .into_iter()
        .filter(move |item| item.status == "ASSIGNED" && item.account_id == candidate_account_id)
        .map(|item| normalize_searched_username(&item.username))
}

/// `/usernames/search` decodes the discriminator as a number and therefore
/// renders `01` as `1`. V1 Lite usernames always use exactly two digits.
fn normalize_searched_username(username: &str) -> String {
    let Some((base, digits)) = username.rsplit_once('.') else {
        return username.to_string();
    };
    let Ok(digits) = digits.parse::<u8>() else {
        return username.to_string();
    };
    if !(1..=99).contains(&digits) {
        return username.to_string();
    }
    format!("{base}.{digits:02}")
}

/// Probes the dotNS contracts for the bare root and the network's RFC-0022
/// `uid.<suffix>` identity account. Prints any recorded usernames. Used to
/// confirm a pre-onboarded account.
pub async fn check_identity(network: NetworkConfig, entropy: &[u8]) -> Result<()> {
    let root = derive_root_keypair_from_entropy(entropy)
        .map_err(|err| anyhow::anyhow!("invalid entropy: {err}"))?;
    let identity = derive_identity_keypair(entropy, network.network_suffix)
        .map_err(|err| anyhow::anyhow!("uid identity derivation failed: {err}"))?;
    let mut reader = AssetHubReader::connect(network.asset_hub_ws).await?;
    let identity_path = format!(
        "//product//{}/index_bytes(0)",
        identity_product_id(network.network_suffix)
    );

    for (label, public) in [
        ("<root>", root.public.to_bytes()),
        (identity_path.as_str(), identity.public.to_bytes()),
    ] {
        let address = product_public_key_to_address(public);
        match reader.dotns_identity(&public).await {
            Ok(identity) => {
                match identity.full_username.or(identity.lite_username) {
                    Some(username) => {
                        println!(
                            "IDENTITY_FOUND path={label} account={address} username={username}"
                        )
                    }
                    None => println!("IDENTITY_NONE path={label} account={address}"),
                }
            }
            Err(err) => println!("IDENTITY_ERROR path={label} account={address} error={err:#}"),
        }
    }
    Ok(())
}

async fn fetch_verifier(client: &reqwest::Client, backend_base: &str) -> Result<[u8; 32]> {
    let url = format!("{backend_base}/attester");
    let body: Value = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()?
        .json()
        .await
        .context("decoding attester response")?;
    let hex_value = body
        .get("attester")
        .and_then(Value::as_str)
        .context("attester response missing 'attester' field")?;
    let bytes = hex::decode(hex_value.strip_prefix("0x").unwrap_or(hex_value))
        .context("attester is not valid hex")?;
    <[u8; 32]>::try_from(bytes)
        .map_err(|bytes| anyhow::anyhow!("attester must be 32 bytes, got {}", bytes.len()))
}

/// `POST /usernames` for `reg`, authenticated as the candidate account the
/// registration was built for (`config.entropy` on `config.network_suffix`).
async fn submit_registration(
    client: &reqwest::Client,
    config: &AttestConfig,
    signed_at: u64,
    reg: &truapi::host_logic::attestation::LiteRegistration,
) -> Result<()> {
    let backend_base = config.backend_base.as_str();
    let url = format!("{backend_base}/usernames");
    let mut dotns = json!({
        "signature": hex0x(&reg.dotns_signature),
        "signedAt": signed_at,
    });
    if let Some(reserved) = config.reserved_username.as_deref() {
        dotns["reservedUsername"] = json!(reserved);
    }
    let body = json!({
        "username": config.username_base,
        "candidateAccountId": reg.candidate_account_id,
        "candidateSignature": hex0x(&reg.candidate_signature),
        "ringVrfKey": hex0x(&reg.ring_vrf_key),
        "proofOfOwnership": hex0x(&reg.proof_of_ownership),
        "identifierKey": hex0x(&reg.identifier_key),
        "consumerRegistrationSignature": hex0x(&reg.consumer_registration_signature),
        "dotns": dotns,
    });
    let response = send_with_backend_auth(
        client,
        backend_base,
        &config.entropy,
        &config.network_suffix,
        |token| client.post(&url).bearer_auth(token).json(&body),
    )
    .await
    .with_context(|| format!("POST {url}"))?;
    let status = response.status();
    if status.is_success() {
        let text = response.text().await.unwrap_or_default();
        debug!(%status, body = %text, "POST /usernames accepted");
        return Ok(());
    }
    let text = response.text().await.unwrap_or_default();
    // Already-registered is a soft success; the on-chain poll confirms it.
    if text.contains("already") || text.contains("AlreadyRegistered") || text.contains("duplicate")
    {
        warn!(%status, body = %text, "username already registered; confirming on-chain");
        return Ok(());
    }
    bail!("username registration failed ({status}): {text}");
}

fn hex0x(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

async fn wait_for_dotns_username(
    reader: &mut AssetHubReader,
    candidate: &[u8; 32],
) -> Result<truapi::host_logic::dotns_gateway::DotnsIdentity> {
    // First-time lite registration is backend-async and can lag the HTTP
    // response. The record is permanent once written. Later runs therefore
    // resolve on the first poll.
    const MAX_ATTEMPTS: usize = 30;
    for attempt in 1..=MAX_ATTEMPTS {
        match reader.dotns_identity(candidate).await {
            Ok(identity) if identity.lite_username.is_some() => {
                crate::terminal_ui::update_activity(
                    "signer",
                    "Setting up signer",
                    Some("dotNS identity ready".to_string()),
                    crate::terminal_ui::ActivityState::Running,
                );
                return Ok(identity);
            }
            Ok(_) => {
                crate::terminal_ui::update_activity(
                    "signer",
                    "Setting up signer",
                    Some(format!(
                        "Waiting for dotNS username · attempt {attempt}/{MAX_ATTEMPTS}"
                    )),
                    crate::terminal_ui::ActivityState::Running,
                );
                debug!("dotNS username poll {attempt}/{MAX_ATTEMPTS}: empty");
            }
            Err(err) => warn!(%err, "dotNS username poll attempt {attempt} failed"),
        }
        if attempt < MAX_ATTEMPTS {
            tokio::time::sleep(Duration::from_secs(4)).await;
        }
    }
    bail!("dotNS username did not appear on Asset Hub after attestation")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    use tokio::net::TcpListener;

    /// The availability response is a flat `{base: status}` map.
    #[test]
    fn availability_reads_flat_and_versioned_status_maps() {
        let flat = json!({ "pntest": "EXHAUSTED", "other": "AVAILABLE" });
        assert_eq!(availability_status(&flat, "pntest"), Some("EXHAUSTED"));
        assert_eq!(availability_status(&flat, "other"), Some("AVAILABLE"));
        assert_eq!(availability_status(&flat, "missing"), None);

        let versioned = json!({
            "_tag": "v1",
            "value": {
                "pntest": { "status": "AVAILABLE", "availableDigits": [1, 2, 3] }
            }
        });
        assert_eq!(availability_status(&versioned, "pntest"), Some("AVAILABLE"));
        assert_eq!(availability_status(&versioned, "missing"), None);
    }

    #[test]
    fn search_username_restores_the_v1_two_digit_discriminator() {
        assert_eq!(normalize_searched_username("alice.1"), "alice.01");
        assert_eq!(normalize_searched_username("alice.42"), "alice.42");
        assert_eq!(normalize_searched_username("fullperson"), "fullperson");
        assert_eq!(normalize_searched_username("future.123"), "future.123");
    }

    #[test]
    fn backend_search_uses_only_assigned_rows_for_the_candidate() -> Result<()> {
        let page: UsernameSearchPage = serde_json::from_value(json!({
            "usernames": [
                { "accountId": "candidate", "username": "alice.1", "status": "ASSIGNED" },
                { "accountId": "candidate", "username": "stale.2", "status": "FAILED" },
                { "accountId": "someone-else", "username": "other.3", "status": "ASSIGNED" }
            ],
            "nextCursor": null
        }))?;

        assert_eq!(
            matching_assigned_usernames(page.usernames, "candidate").collect::<Vec<_>>(),
            ["alice.01"]
        );
        Ok(())
    }

    #[tokio::test]
    async fn username_registration_authenticates_as_the_candidate() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let backend_base = format!("http://{}/api/v1", listener.local_addr()?);
        let server = tokio::spawn(serve_candidate_bound_registration(listener));

        let entropy = [7u8; 16];
        let registration =
            build_lite_registration(&entropy, "paseo", [9u8; 32], "testing", None, 123)?;
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        let config = AttestConfig {
            backend_base: backend_base.clone(),
            asset_hub_ws: String::new(),
            network_suffix: "paseo".to_string(),
            entropy: entropy.to_vec(),
            username_base: "testing".to_string(),
            reserved_username: None,
        };
        let result = submit_registration(&client, &config, 123, &registration).await;
        let requests = server.await??;

        assert_eq!(requests.len(), 3);
        assert!(
            result.is_ok(),
            "candidate-bound backend rejected registration: {}",
            result.unwrap_err()
        );
        Ok(())
    }

    async fn serve_candidate_bound_registration(listener: TcpListener) -> Result<Vec<String>> {
        let mut requests = Vec::new();
        let mut authenticated_account = None;
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().await?;
            let mut buffer = vec![0u8; 32 * 1024];
            let read = stream.read(&mut buffer).await?;
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let lowercase = request.to_ascii_lowercase();
            let (status, body) = if request.starts_with("POST /api/v1/auth/challenges ") {
                ("200 OK", r#"{"challenge":"Y2hhbGxlbmdl"}"#.to_string())
            } else if request.starts_with("POST /api/v1/auth/token ") {
                let client_id = request_header(&request, "Auth-ClientId")
                    .context("auth token request missing Auth-ClientId")?;
                let bytes = BASE64
                    .decode(client_id)
                    .context("Auth-ClientId is not base64")?;
                let public_key: [u8; 32] = bytes.try_into().map_err(|bytes: Vec<u8>| {
                    anyhow::anyhow!("Auth-ClientId is {} bytes", bytes.len())
                })?;
                authenticated_account = Some(product_public_key_to_address(public_key));
                ("200 OK", r#"{"token":"candidate-token"}"#.to_string())
            } else if request.starts_with("POST /api/v1/usernames ")
                && lowercase.contains("authorization: bearer candidate-token")
            {
                let payload = request
                    .split_once("\r\n\r\n")
                    .map(|(_, body)| body)
                    .context("username request missing body")?;
                let candidate = serde_json::from_str::<Value>(payload)?
                    .get("candidateAccountId")
                    .and_then(Value::as_str)
                    .context("username request missing candidateAccountId")?
                    .to_string();
                if authenticated_account.as_deref() == Some(candidate.as_str()) {
                    ("202 Accepted", r#"{"status":"PENDING"}"#.to_string())
                } else {
                    (
                        "403 Forbidden",
                        r#"{"error":"candidateAccountId must be the authenticated account."}"#
                            .to_string(),
                    )
                }
            } else {
                (
                    "500 Internal Server Error",
                    r#"{"error":"unexpected request"}"#.to_string(),
                )
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await?;
            requests.push(request);
        }
        Ok(requests)
    }

    fn request_header<'a>(request: &'a str, expected: &str) -> Option<&'a str> {
        request.lines().find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case(expected).then(|| value.trim())
        })
    }

    #[test]
    fn rejected_token_eviction_is_scoped_to_the_matching_cached_value() {
        const BASE: &str = "test://rejected-token";
        const OTHER_BASE: &str = "test://other-token";
        const SUBJECT: [u8; 32] = [1; 32];
        const OTHER_SUBJECT: [u8; 32] = [2; 32];
        let mut tokens = BACKEND_TOKENS
            .lock()
            .expect("backend token cache mutex poisoned");
        tokens.retain(|token| token.backend_base != BASE && token.backend_base != OTHER_BASE);
        tokens.push(CachedBackendToken {
            backend_base: BASE.to_string(),
            auth_client_id: SUBJECT,
            value: "stale".to_string(),
        });
        tokens.push(CachedBackendToken {
            backend_base: BASE.to_string(),
            auth_client_id: OTHER_SUBJECT,
            value: "other-subject".to_string(),
        });
        tokens.push(CachedBackendToken {
            backend_base: OTHER_BASE.to_string(),
            auth_client_id: SUBJECT,
            value: "keep".to_string(),
        });
        drop(tokens);

        evict_rejected_backend_token(BASE, &SUBJECT, "newer-token");
        assert!(
            BACKEND_TOKENS
                .lock()
                .expect("backend token cache mutex poisoned")
                .iter()
                .any(|token| {
                    token.backend_base == BASE
                        && token.auth_client_id == SUBJECT
                        && token.value == "stale"
                })
        );

        evict_rejected_backend_token(BASE, &SUBJECT, "stale");
        let mut tokens = BACKEND_TOKENS
            .lock()
            .expect("backend token cache mutex poisoned");
        assert!(
            tokens
                .iter()
                .all(|token| token.backend_base != BASE || token.auth_client_id != SUBJECT)
        );
        assert!(tokens.iter().any(|token| {
            token.backend_base == BASE
                && token.auth_client_id == OTHER_SUBJECT
                && token.value == "other-subject"
        }));
        assert!(tokens.iter().any(|token| {
            token.backend_base == OTHER_BASE
                && token.auth_client_id == SUBJECT
                && token.value == "keep"
        }));
        tokens.retain(|token| token.backend_base != BASE && token.backend_base != OTHER_BASE);
    }

    #[tokio::test]
    async fn cached_401_reauthenticates_and_retries_once() -> Result<()> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let backend_base = format!("http://{}/api/v1", listener.local_addr()?);
        let server = tokio::spawn(serve_auth_retry(listener));
        let entropy = [11u8; 16];
        let auth_client_id = derive_identity_keypair(&entropy, "paseo")
            .map_err(|err| anyhow::anyhow!("derive test auth identity: {err}"))?
            .public
            .to_bytes();

        {
            let mut tokens = BACKEND_TOKENS
                .lock()
                .expect("backend token cache mutex poisoned");
            tokens.retain(|token| token.backend_base != backend_base);
            tokens.push(CachedBackendToken {
                backend_base: backend_base.clone(),
                auth_client_id,
                value: "expired-test-token".to_string(),
            });
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()?;
        let url = format!("{backend_base}/protected");
        let response = send_with_backend_auth(&client, &backend_base, &entropy, "paseo", |token| {
            client.post(&url).bearer_auth(token).body("{}")
        })
        .await?;
        assert_eq!(response.status(), reqwest::StatusCode::OK);
        let requests = server.await??;
        assert_eq!(requests.len(), 4);
        assert!(requests[0].contains("authorization: Bearer expired-test-token"));
        assert!(requests[1].starts_with("POST /api/v1/auth/challenges "));
        assert!(requests[2].starts_with("POST /api/v1/auth/token "));
        let encoded_client_id = BASE64.encode(auth_client_id);
        assert_eq!(
            request_header(&requests[2], "Auth-ClientId"),
            Some(encoded_client_id.as_str())
        );
        assert!(requests[3].contains("authorization: Bearer fresh-token"));

        let mut tokens = BACKEND_TOKENS
            .lock()
            .expect("backend token cache mutex poisoned");
        assert!(tokens.iter().any(|token| {
            token.backend_base == backend_base
                && token.auth_client_id == auth_client_id
                && token.value == "fresh-token"
        }));
        tokens.retain(|token| token.backend_base != backend_base);
        Ok(())
    }

    async fn serve_auth_retry(listener: TcpListener) -> Result<Vec<String>> {
        let mut requests = Vec::new();
        for _ in 0..4 {
            let (mut stream, _) = listener.accept().await?;
            let mut buffer = vec![0u8; 16 * 1024];
            let read = stream.read(&mut buffer).await?;
            let request = String::from_utf8_lossy(&buffer[..read]).to_string();
            let lowercase = request.to_ascii_lowercase();
            let (status, body) = if request.starts_with("POST /api/v1/protected ")
                && lowercase.contains("authorization: bearer expired-test-token")
            {
                ("401 Unauthorized", r#"{"title":"Invalid Token"}"#)
            } else if request.starts_with("POST /api/v1/auth/challenges ") {
                ("200 OK", r#"{"challenge":"Y2hhbGxlbmdl"}"#)
            } else if request.starts_with("POST /api/v1/auth/token ") {
                ("200 OK", r#"{"token":"fresh-token"}"#)
            } else if request.starts_with("POST /api/v1/protected ")
                && lowercase.contains("authorization: bearer fresh-token")
            {
                ("200 OK", r#"{"ok":true}"#)
            } else {
                (
                    "500 Internal Server Error",
                    r#"{"error":"unexpected request"}"#,
                )
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(response.as_bytes()).await?;
            requests.push(request);
        }
        Ok(requests)
    }
}
