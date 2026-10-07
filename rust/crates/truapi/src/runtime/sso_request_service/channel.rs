//! SSO statement-store channel to the paired remote signing host.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::super::sso_remote::{
    RemoteResponseWait, SSO_LOCAL_DISCONNECT_REASON, SsoRemoteResponseError, SsoSessionKey,
    fresh_statement_expiry, reply_matcher, sso_message_id, statement_subscription_stream,
    subscribe_statement_topic, wait_for_sso_remote_response,
};
use super::super::statement_store_rpc::{self, StatementStoreRpc};
use super::SsoRequestService;
use crate::host_internal::sso_messages::{
    RemoteMessage, RemoteMessageData, SsoSessionStatement, Withdrawal,
    build_outgoing_request_statement, decode_sso_session_statement, v1,
};
use crate::host_internal::sso_wire::SsoRequest;
use crate::host_logic::session::{SessionInfo, SsoSessionInfo};
use crate::host_logic::statement_store::parse_new_statements_result;

use futures::FutureExt;
use futures::future::{AbortHandle, Abortable};
use tracing::{debug, instrument, warn};
use truapi::{CallContext, CancellationReason};

/// Active peer-disconnect watcher for one SSO session; aborts on drop.
pub struct SsoDisconnectMonitor {
    key: SsoSessionKey,
    abort: AbortHandle,
}

impl Drop for SsoDisconnectMonitor {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

struct RequestWithdrawal<'a> {
    service: &'a SsoRequestService,
    session: &'a SsoSessionInfo,
    message_id: &'a str,
    call: &'a CallContext,
    submitting: &'a AtomicBool,
    armed: bool,
}

impl Drop for RequestWithdrawal<'_> {
    fn drop(&mut self) {
        if self.armed
            && self.call.cancel().reason() == Some(CancellationReason::Cancelled)
            && self.submitting.load(Ordering::Acquire)
            && SsoSessionKey::from_session(self.session).matches(&self.service.session_state)
        {
            self.service.withdraw_request(self.session, self.message_id);
        }
    }
}

impl SsoRequestService {
    /// Watch the session's topics for a peer disconnect statement, replacing
    /// any monitor for a different session. No-op when one is already running
    /// for this session.
    pub fn start_disconnect_monitor(&self, session: &SessionInfo) {
        let Some(sso) = session.sso.clone() else {
            return;
        };
        let key = SsoSessionKey::from_session(&sso);

        let (registration, spawner, previous) = {
            let _lifecycle = self.grants.lifecycle();
            if !self.current_sso_session_matches(key) {
                return;
            }
            let mut current = self
                .disconnect_monitor
                .lock()
                .expect("SSO disconnect monitor mutex poisoned");
            if current.as_ref().is_some_and(|active| active.key == key) {
                return;
            }
            let (abort, registration) = AbortHandle::new_pair();
            let previous = current.replace(SsoDisconnectMonitor { key, abort });
            (registration, self.spawner.clone(), previous)
        };
        drop(previous);

        let statement_store = self.statement_store.clone();
        let service = self.weak_self.clone();
        let future = async move {
            let result = wait_for_sso_peer_disconnect(statement_store, sso).await;
            let Some(service) = service.upgrade() else {
                return;
            };
            {
                let mut active = service
                    .disconnect_monitor
                    .lock()
                    .expect("SSO disconnect monitor mutex poisoned");
                if active.as_ref().is_some_and(|active| active.key == key) {
                    *active = None;
                }
            }
            match result {
                Ok(()) => {
                    service.handle_signing_host_disconnected(key).await;
                }
                Err(reason) => {
                    warn!(%reason, "SSO peer disconnect monitor stopped");
                }
            }
        };
        spawner(Box::pin(Abortable::new(future, registration).map(|_| ())));
    }

    /// Detach channel state while the session lifecycle is locked.
    pub fn detach_session_channel(
        &self,
        session: Option<&SessionInfo>,
    ) -> Option<SsoDisconnectMonitor> {
        *self
            .newest_request
            .lock()
            .expect("newest request mutex poisoned") = None;
        self.grants.clear_statement_store_allowance_keys(session);
        self.grants.clear_bulletin_allowance_keys(session);
        self.grants.clear_product_subtrees(session);
        self.disconnect_monitor
            .lock()
            .expect("SSO disconnect monitor mutex poisoned")
            .take()
    }

    /// Wake detached channel work after releasing the lifecycle lock.
    pub fn stop_session_channel(
        &self,
        session: Option<&SessionInfo>,
        monitor: Option<SsoDisconnectMonitor>,
    ) {
        drop(monitor);
        if let Some(sso) = session.and_then(|session| session.sso.as_ref()) {
            self.session_disconnects
                .notify(sso, SSO_LOCAL_DISCONNECT_REASON);
        }
    }

    /// Best-effort `Disconnected` notification to the SSO peer.
    #[instrument(skip_all, fields(runtime.method = "sso.disconnect.submit"))]
    pub async fn submit_disconnected_message(&self, session: &SessionInfo) -> Result<(), String> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| "No SSO session state".to_string())?;
        let message_id = sso_message_id();
        let message = RemoteMessage {
            message_id: message_id.clone(),
            data: RemoteMessageData::V1(v1::RemoteMessage::Disconnected),
        };
        let statement = self.build_request_channel_statement(sso, message_id, message, None)?;
        self.statement_store
            .submit_fire_and_forget(statement, "SSO statement-store")
            .await
            .map_err(|err| format!("SSO statement submit failed: {err}"))?;
        Ok(())
    }

    /// Build a statement on the session's request channel, recording
    /// `newest` as the request the channel now carries.
    ///
    /// The store keeps one statement per channel and the later build wins, so
    /// the record and the build share one lock.
    fn build_request_channel_statement(
        &self,
        sso: &SsoSessionInfo,
        statement_request_id: String,
        message: RemoteMessage,
        newest: Option<String>,
    ) -> Result<Vec<u8>, String> {
        let mut newest_request = self
            .newest_request
            .lock()
            .expect("newest request mutex poisoned");
        let statement = build_outgoing_request_statement(
            sso,
            statement_request_id,
            vec![message],
            fresh_statement_expiry(),
        )?;
        *newest_request = newest;
        Ok(statement)
    }

    /// Withdraw the request sent as `message_id` from the paired host.
    ///
    /// A `Cancel` replaces the newest statement on the request channel, so it
    /// is sent only while that is the request it names; otherwise it would
    /// replace another request instead. Sent in the background, so it neither
    /// holds up the withdrawn call's answer nor dies with its unwind grace.
    fn withdraw_request(&self, sso: &SsoSessionInfo, message_id: &str) {
        let withdrawal = {
            let mut newest_request = self
                .newest_request
                .lock()
                .expect("newest request mutex poisoned");
            if newest_request.as_deref() != Some(message_id) {
                return;
            }
            let cancel_id = sso_message_id();
            let message = RemoteMessage {
                message_id: cancel_id.clone(),
                data: RemoteMessageData::V1(v1::RemoteMessage::Cancel(Withdrawal {
                    message_id: message_id.to_string(),
                })),
            };
            *newest_request = None;
            build_outgoing_request_statement(
                sso,
                cancel_id,
                vec![message],
                fresh_statement_expiry(),
            )
        };
        let statement_store = self.statement_store.clone();
        let message_id = message_id.to_string();
        (self.spawner)(Box::pin(async move {
            let submitted = match withdrawal {
                Ok(statement) => statement_store
                    .submit_fire_and_forget(statement, "SSO statement-store")
                    .await
                    .map_err(|err| err.to_string()),
                Err(reason) => Err(reason),
            };
            if let Err(reason) = submitted {
                warn!(%message_id, %reason, "could not withdraw the SSO request");
            }
        }));
    }

    /// Send `request` to the paired signing host and await its typed answer.
    ///
    /// The outer error is the transport's; the inner result is the peer's
    /// payload for this request type.
    #[instrument(skip_all, fields(runtime.method = "sso.remote_message.submit", action = R::NAME))]
    pub async fn call<R: SsoRequest>(
        &self,
        cx: &CallContext,
        session: &SessionInfo,
        request: R,
    ) -> Result<R::Response, SsoRemoteResponseError> {
        let sso = session
            .sso
            .as_ref()
            .ok_or_else(|| SsoRemoteResponseError::Failure("No SSO session state".to_string()))?;
        let key = SsoSessionKey::from_session(sso);
        let (_disconnect_guard, disconnect) = self.session_disconnects.subscribe(sso);
        if !key.matches(&self.session_state) {
            return Err(SsoRemoteResponseError::LocalDisconnected);
        }
        let message_id = sso_message_id();
        let statement = self
            .build_request_channel_statement(
                sso,
                message_id.clone(),
                RemoteMessage::request(message_id.clone(), request),
                Some(message_id.clone()),
            )
            .map_err(SsoRemoteResponseError::Failure)?;
        let rpc_client = self
            .statement_store
            .client("SSO statement-store")
            .await
            .map_err(|err| SsoRemoteResponseError::Failure(err.to_string()))?;
        let own_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_own)
            .await
            .map_err(|err| {
                SsoRemoteResponseError::Failure(format!(
                    "SSO own statement-store subscribe failed: {err}"
                ))
            })?;
        let peer_subscription = subscribe_statement_topic(&rpc_client, sso.session_id_peer)
            .await
            .map_err(|err| {
                SsoRemoteResponseError::Failure(format!(
                    "SSO peer statement-store subscribe failed: {err}"
                ))
            })?;
        let submit_client = rpc_client.clone();
        let session_state = self.session_state.clone();
        let submitting = Arc::new(AtomicBool::new(false));
        let submit_started = submitting.clone();
        let submit = async move {
            if !key.matches(&session_state) {
                return Err(SsoRemoteResponseError::LocalDisconnected);
            }
            submit_started.store(true, Ordering::Release);
            statement_store_rpc::submit_sso(&submit_client, statement, "pairing-host request")
                .await
                .map_err(|err| {
                    SsoRemoteResponseError::Failure(format!("SSO statement submit failed: {err}"))
                })
        }
        .boxed();
        let action = R::NAME;
        debug!(action, %message_id, "submitted SSO remote message, awaiting response");
        let mut withdrawal = RequestWithdrawal {
            service: self,
            session: sso,
            message_id: &message_id,
            call: cx,
            submitting: &submitting,
            armed: true,
        };
        let result = wait_for_sso_remote_response(
            RemoteResponseWait {
                own_statements: statement_subscription_stream(own_subscription, "own"),
                peer_statements: statement_subscription_stream(peer_subscription, "peer"),
                submit,
                session: sso,
                statement_request_id: &message_id,
                remote_message_id: &message_id,
                cancel: cx.cancel(),
                disconnect: Some(disconnect),
            },
            reply_matcher::<R>(&message_id),
        )
        .await;
        withdrawal.armed = matches!(
            &result,
            Err(SsoRemoteResponseError::Cancelled(error))
                if error.reason() == CancellationReason::Cancelled
        );
        let result = result.map_err(|reason| match reason {
            SsoRemoteResponseError::Cancelled(err) if !cx.request_id().is_empty() => {
                SsoRemoteResponseError::Cancelled(err.with_remote_message_id(cx.request_id()))
            }
            reason => reason,
        });
        match &result {
            Ok(_) => debug!(action, %message_id, "SSO remote response received"),
            Err(reason) => warn!(action, %message_id, %reason, "SSO remote message failed"),
        }
        if matches!(&result, Err(SsoRemoteResponseError::PeerDisconnected)) {
            self.handle_signing_host_disconnected(key).await;
        }
        result.map(|response| response.payload)
    }
}

#[instrument(skip_all, fields(runtime.method = "sso.peer_disconnect.monitor"))]
async fn wait_for_sso_peer_disconnect(
    statement_store: StatementStoreRpc,
    session: SsoSessionInfo,
) -> Result<(), String> {
    let rpc_client = statement_store
        .client("SSO disconnect monitor")
        .await
        .map_err(|err| err.to_string())?;
    let mut subscription =
        statement_store_rpc::subscribe_match_all(&rpc_client, &[session.session_id_peer])
            .await
            .map_err(|err| format!("SSO disconnect monitor subscribe failed: {err}"))?;
    while let Some(item) = subscription.next().await {
        let value = item.map_err(|err| format!("SSO disconnect monitor item failed: {err}"))?;
        let page = parse_new_statements_result("sso-peer-disconnect-monitor".to_string(), &value)
            .map_err(|err| err.to_string())?;
        for statement in page.statements {
            let Some(SsoSessionStatement::RemoteMessages(messages)) = decode_sso_session_statement(
                &session,
                &statement,
                "truapi:sso-peer-disconnect-monitor",
            )?
            else {
                continue;
            };
            for message in messages {
                if message? == v1::RemoteMessage::Disconnected {
                    return Ok(());
                }
            }
        }
    }
    Err("SSO disconnect monitor response stream ended".to_string())
}
