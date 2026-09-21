// SPDX-License-Identifier: AGPL-3.0-only
//! Host-private HOP connections: trusted endpoints, current session and grants.

use std::{future::Future, sync::Arc, time::Duration};

use futures::{FutureExt, future::Either};
use futures_timer::Delay;
use serde_json::Value;
use subxt_rpcs::client::RpcClientT;

use super::{
    ChatError, NativeChatContext,
    background::{require_authorized, require_upload_authorized},
    hop::{HopError, HopRpc},
};
use crate::host_rpc_client::HostRpcClient;

const RPC_TIMEOUT: Duration = Duration::from_secs(30);

pub(super) struct SessionHopRpc {
    context: NativeChatContext,
    product: String,
    rpc: HostRpcClient,
}

async fn bounded<T>(future: impl Future<Output = T>) -> Result<T, ChatError> {
    let future = future.fuse();
    let timeout = Delay::new(RPC_TIMEOUT).fuse();
    futures::pin_mut!(future, timeout);
    match futures::future::select(future, timeout).await {
        Either::Left((result, _)) => Ok(result),
        Either::Right(_) => Err(ChatError::NetworkUnavailable),
    }
}

impl SessionHopRpc {
    pub(super) async fn connect(
        context: &NativeChatContext,
        product: &str,
        endpoint: &str,
    ) -> Result<Self, ChatError> {
        require_authorized(context, product).await?;
        let platform = context.services.platform.as_ref();
        let genesis = context.services.bulletin.genesis_hash();
        let allowed = bounded(platform.allowed_hop_endpoints(genesis))
            .await?
            .map_err(|_| ChatError::NetworkUnavailable)?;
        truapi_platform::ensure_allowed_hop_endpoint(endpoint, &allowed)
            .map_err(|_| ChatError::InvalidStatement)?;
        require_authorized(context, product).await?;
        let connection = bounded(platform.connect_hop(genesis, endpoint.to_owned()))
            .await?
            .map_err(|_| ChatError::NetworkUnavailable)?;
        // Establish response-pump ownership before the next await, so failed
        // authorization drops and closes a successfully opened connection.
        let rpc = HostRpcClient::new(Arc::from(connection), context.services.spawner.clone());
        require_authorized(context, product).await?;
        Ok(Self {
            context: context.clone(),
            product: product.to_owned(),
            rpc,
        })
    }
}

#[async_trait::async_trait]
impl HopRpc for SessionHopRpc {
    async fn call(&self, method: &str, params: Value) -> Result<Value, HopError> {
        let authorize = || async {
            if method == "hop_submit" {
                require_upload_authorized(&self.context, &self.product).await
            } else {
                require_authorized(&self.context, &self.product).await
            }
        };
        authorize()
            .await
            .map_err(|_| HopError::Transport("Chat authorization ended".into()))?;
        if !params.is_array() && !params.is_object() {
            return Err(HopError::Codec(
                "RPC parameters must be an array or object".into(),
            ));
        }
        let encoded = serde_json::value::to_raw_value(&params)
            .map_err(|_| HopError::Codec("invalid RPC parameters".into()))?;
        let result = bounded(self.rpc.request_raw(method, Some(encoded)))
            .await
            .map_err(|_| HopError::Transport("HOP request timed out".into()))?;
        authorize()
            .await
            .map_err(|_| HopError::Transport("Chat authorization ended".into()))?;
        let raw = result.map_err(|error| match error {
            // Upstream text is not trusted and may echo request material.
            subxt_rpcs::Error::User(error) => HopError::Rpc {
                code: i64::from(error.code),
                message: "remote HOP request failed".into(),
            },
            _ => HopError::Transport("HOP connection failed".into()),
        })?;
        serde_json::from_str(raw.get()).map_err(|_| HopError::Codec("invalid RPC result".into()))
    }
}
