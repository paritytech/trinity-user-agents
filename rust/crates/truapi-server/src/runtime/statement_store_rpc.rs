//! Runtime helper for People-chain statement-store JSON-RPC.
//!
//! Statement traffic opens short-lived host RPC connections for its own
//! subscription lifetimes instead of riding the shared
//! [`crate::chain_runtime::ChainRuntime`] chainHead runtime. If a host shares
//! same-topic statement subscriptions on one upstream connection, the host
//! broker must fan out and ref-count same-upstream-token notifications.

use std::sync::Arc;

use core::time::Duration;
use serde_json::{Value, json};

use subxt_rpcs::RpcClient;
use subxt_rpcs::client::{RpcSubscription, rpc_params};
use thiserror::Error;
use tracing::warn;
use truapi::latest::GenericError;
use truapi_platform::{JsonRpcConnection, Platform};

use crate::host_logic::statement_store::{
    SUBMIT_STATEMENT_METHOD, SUBSCRIBE_STATEMENT_METHOD, TopicFilterKind,
    UNSUBSCRIBE_STATEMENT_METHOD, hex_topic,
};
use crate::host_rpc_client::HostRpcClient;
use crate::subscription::Spawner;

const SSO_NO_ALLOWANCE_RETRY_ATTEMPTS: usize = 5;
const SSO_NO_ALLOWANCE_RETRY_DELAY: Duration = Duration::from_secs(1);

/// Error opening a statement-store RPC client over the host platform.
#[derive(Debug, Error)]
pub(crate) enum StatementStoreRpcClientError {
    /// The host failed to open a People-chain JSON-RPC connection.
    #[error("{label} connect failed: {error:?}")]
    Connect {
        /// Operation label requesting the connection.
        label: &'static str,
        /// Host platform error.
        error: GenericError,
    },
}

/// People-chain statement-store RPC client factory.
#[derive(Clone)]
pub(crate) struct StatementStoreRpc {
    platform: Arc<dyn Platform>,
    people_chain_genesis_hash: [u8; 32],
    spawner: Spawner,
}

impl StatementStoreRpc {
    /// Build a helper backed by the platform-owned chain provider.
    pub(super) fn new(
        platform: Arc<dyn Platform>,
        people_chain_genesis_hash: [u8; 32],
        spawner: Spawner,
    ) -> Self {
        Self {
            platform,
            people_chain_genesis_hash,
            spawner,
        }
    }

    /// Open a People-chain RPC client already scoped to its genesis hash, for
    /// allowance paths that key the chain-context cache by it.
    pub(crate) async fn chain_client(
        &self,
        label: &'static str,
    ) -> Result<crate::runtime::statement_allowance::ChainClient, StatementStoreRpcClientError>
    {
        Ok(crate::runtime::statement_allowance::ChainClient::new(
            crate::runtime::statement_allowance::rpc::RpcClient::new(self.client(label).await?),
            self.people_chain_genesis_hash,
        ))
    }

    /// Open a statement-store RPC client over the host-provided People-chain
    /// connection.
    pub(super) async fn client(
        &self,
        label: &'static str,
    ) -> Result<RpcClient, StatementStoreRpcClientError> {
        let connection = self.connect(label).await?;
        Ok(RpcClient::new(HostRpcClient::new(
            connection,
            self.spawner.clone(),
        )))
    }

    /// Submit a SCALE-encoded statement and wait for the JSON-RPC ack.
    pub(super) async fn submit(
        &self,
        statement: Vec<u8>,
        label: &'static str,
    ) -> Result<(), StatementSubmitError> {
        let rpc_client = self
            .client(label)
            .await
            .map_err(|err| StatementSubmitError::Rpc(err.to_string()))?;
        submit(&rpc_client, statement).await
    }

    /// Submit an SSO statement, tolerating the short propagation window after
    /// an allowance registration is included but not yet visible to the
    /// Statement Store RPC backend.
    pub(super) async fn submit_sso(
        &self,
        statement: Vec<u8>,
        label: &'static str,
    ) -> Result<(), String> {
        let rpc_client = self.client(label).await.map_err(|err| err.to_string())?;
        submit_sso(&rpc_client, statement, label).await
    }

    /// Submit a SCALE-encoded statement without waiting for the JSON-RPC ack.
    pub(super) async fn submit_fire_and_forget(
        &self,
        statement: Vec<u8>,
        label: &'static str,
    ) -> Result<(), String> {
        let connection = self.connect(label).await.map_err(|err| err.to_string())?;
        HostRpcClient::new(connection, self.spawner.clone())
            .send_fire_and_forget(
                SUBMIT_STATEMENT_METHOD,
                rpc_params![format!("0x{}", hex::encode(&statement))].build(),
            )
            .map_err(rpc_error_message)
    }

    async fn connect(
        &self,
        label: &'static str,
    ) -> Result<Arc<dyn JsonRpcConnection>, StatementStoreRpcClientError> {
        self.platform
            .connect(self.people_chain_genesis_hash)
            .await
            .map(Arc::from)
            .map_err(|error| StatementStoreRpcClientError::Connect { label, error })
    }
}

/// Subscribe to statements matching the requested topic filter.
pub(super) async fn subscribe(
    rpc_client: &RpcClient,
    kind: TopicFilterKind,
    topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
    rpc_client
        .subscribe::<Value>(
            SUBSCRIBE_STATEMENT_METHOD,
            rpc_params![filter(kind, topics)],
            UNSUBSCRIBE_STATEMENT_METHOD,
        )
        .await
}

/// Subscribe to statements matching every topic.
pub(super) async fn subscribe_match_all(
    rpc_client: &RpcClient,
    topics: &[[u8; 32]],
) -> Result<RpcSubscription<Value>, subxt_rpcs::Error> {
    subscribe(rpc_client, TopicFilterKind::MatchAll, topics).await
}

#[derive(Debug, Error)]
pub(super) enum StatementSubmitError {
    #[error("{0}")]
    Rpc(String),
    #[error("statement_submit not accepted: {0}")]
    Rejected(Value),
}

impl StatementSubmitError {
    pub(super) fn is_no_allowance(&self) -> bool {
        matches!(self, Self::Rejected(result)
            if result["status"] == "rejected" && result["reason"] == "noAllowance")
    }
}

/// Submit a SCALE-encoded statement and confirm the store accepted it.
///
/// `statement_submit` returns an RPC error only for internal failures; a
/// rejected or invalid statement (e.g. `NoAllowance`, `BadProof`) comes back as
/// `Ok(SubmitResult)`. Treat only `new`/`known` as success, so allowance/proof
/// rejections surface instead of being silently dropped.
pub(super) async fn submit(
    rpc_client: &RpcClient,
    statement: Vec<u8>,
) -> Result<(), StatementSubmitError> {
    let result = rpc_client
        .request::<Value>(
            SUBMIT_STATEMENT_METHOD,
            rpc_params![format!("0x{}", hex::encode(&statement))],
        )
        .await
        .map_err(|error| StatementSubmitError::Rpc(rpc_error_message(error)))?;
    match result.get("status").and_then(Value::as_str) {
        Some("new") | Some("known") => Ok(()),
        _ => Err(StatementSubmitError::Rejected(result)),
    }
}

pub(super) async fn submit_sso(
    rpc_client: &RpcClient,
    statement: Vec<u8>,
    label: &'static str,
) -> Result<(), String> {
    for attempt in 1..=SSO_NO_ALLOWANCE_RETRY_ATTEMPTS {
        match submit(rpc_client, statement.clone()).await {
            Ok(()) => return Ok(()),
            Err(reason)
                if reason.is_no_allowance() && attempt < SSO_NO_ALLOWANCE_RETRY_ATTEMPTS =>
            {
                warn!(
                    label,
                    attempt,
                    max_attempts = SSO_NO_ALLOWANCE_RETRY_ATTEMPTS,
                    "SSO allowance not visible yet; retrying statement submission"
                );
                futures_timer::Delay::new(SSO_NO_ALLOWANCE_RETRY_DELAY).await;
            }
            Err(reason) => return Err(reason.to_string()),
        }
    }
    unreachable!("the bounded SSO submit loop always returns")
}

/// Statement-store topic filter encoded as JSON-RPC params.
pub(super) fn filter(kind: TopicFilterKind, topics: &[[u8; 32]]) -> Value {
    let topics = topics.iter().map(hex_topic).collect::<Vec<_>>();
    match kind {
        TopicFilterKind::MatchAll => json!({ "matchAll": topics }),
        TopicFilterKind::MatchAny => json!({ "matchAny": topics }),
    }
}

/// Human-readable JSON-RPC error message, preserving user error text when
/// provided by the remote endpoint.
pub(super) fn rpc_error_message(error: subxt_rpcs::Error) -> String {
    match error {
        subxt_rpcs::Error::User(error) => error.message,
        other => other.to_string(),
    }
}
