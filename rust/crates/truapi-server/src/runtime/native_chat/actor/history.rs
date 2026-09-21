// SPDX-License-Identifier: AGPL-3.0-only
//! Expand authenticated HOP history privately; custody commits precede every ACK.

use std::collections::BTreeSet;

use super::*;
use crate::runtime::chat_device::{
    CompactedHistory, OpenedDeviceMessage, PaymentMemo, classify_message,
};
use crate::runtime::native_chat::{
    hop::{FileTicket, HopClient, HopError, PendingAck},
    hop_access::SessionHopRpc,
};

const MAX_HISTORY_IMPORTS: usize = 4096;
const MAX_EXPANDED_BYTES: usize = 16 * 1024 * 1024;
const MAX_HISTORY_DEPTH: usize = 64;

#[derive(Clone, Encode, Decode)]
struct HistoryAck {
    endpoint: String,
    ticket: Secret32,
    encoded: Vec<u8>,
}

#[derive(Clone, Encode, Decode)]
pub(super) struct HistoryImport {
    peer: [u8; 32],
    digest: [u8; 32],
    pending: Option<HistoryAck>,
}

pub(super) struct ExpandedHistory {
    pub ordinary: Vec<Vec<u8>>,
    pub payments: Vec<PaymentMemo>,
    pub rich: Vec<crate::runtime::chat_device::RichContent>,
    pub imports: Vec<HistoryImport>,
}

pub(super) fn reference_digest(reference: &CompactedHistory) -> [u8; 32] {
    let bytes = Zeroizing::new(
        (
            reference.message_id.as_str(),
            reference.timestamp,
            reference.identifier,
            &*reference.ticket,
            reference.endpoint.as_str(),
        )
            .encode(),
    );
    hash(&bytes)
}

pub(super) fn has_pending(imports: &[HistoryImport]) -> bool {
    imports.iter().any(|entry| entry.pending.is_some())
}

pub(super) fn validate_imports(imports: &[HistoryImport]) -> Result<(), Error> {
    if imports.len() > MAX_HISTORY_IMPORTS {
        return Err(Error::StorageUnavailable);
    }
    let mut seen = BTreeSet::new();
    for entry in imports {
        if !seen.insert((entry.peer, entry.digest)) {
            return Err(Error::StorageUnavailable);
        }
        if let Some(pending) = &entry.pending {
            if !pending.endpoint.starts_with("wss://") {
                return Err(Error::StorageUnavailable);
            }
            PendingAck::restore(&pending.encoded).map_err(|_| Error::StorageUnavailable)?;
        }
    }
    Ok(())
}

fn claim_error(error: HopError) -> Error {
    match error {
        HopError::Crypto | HopError::Integrity | HopError::Codec(_) | HopError::InvalidProgress => {
            Error::InvalidStatement
        }
        HopError::Rpc { .. } | HopError::Transport(_) | HopError::NotFound(_) => {
            Error::NetworkUnavailable
        }
    }
}

impl NativeChatActor {
    pub(super) async fn expand_history(
        &self,
        context: &NativeChatContext,
        peer: [u8; 32],
        messages: Vec<OpenedDeviceMessage>,
    ) -> Result<ExpandedHistory, Error> {
        let (mut seen, existing_count) = self
            .store
            .read(|state| {
                (
                    state
                        .history_imports
                        .iter()
                        .filter(|entry| entry.peer == peer)
                        .map(|entry| entry.digest)
                        .collect::<BTreeSet<_>>(),
                    state.history_imports.len(),
                )
            })
            .await?;
        let mut expanded = ExpandedHistory {
            ordinary: Vec::new(),
            payments: Vec::new(),
            rich: Vec::new(),
            imports: Vec::new(),
        };
        let mut work: Vec<_> = messages
            .into_iter()
            .rev()
            .map(|message| (message, 0usize))
            .collect();
        let mut bytes_seen = 0usize;
        while let Some((message, depth)) = work.pop() {
            context.require_current()?;
            match message {
                OpenedDeviceMessage::Ordinary(bytes) => {
                    let decoded =
                        wire::decode_message(&bytes).map_err(|_| Error::InvalidStatement)?;
                    if !super::receive::valid_peer_timestamp(decoded.timestamp, current_unix_secs())
                    {
                        return Err(Error::InvalidStatement);
                    }
                    expanded.ordinary.push(bytes);
                }
                OpenedDeviceMessage::Payment(memo) => {
                    if !super::receive::valid_peer_timestamp(memo.timestamp, current_unix_secs()) {
                        return Err(Error::InvalidStatement);
                    }
                    expanded.payments.push(memo);
                }
                OpenedDeviceMessage::RichContent(message) => {
                    if !super::receive::valid_peer_timestamp(message.timestamp, current_unix_secs())
                    {
                        return Err(Error::InvalidStatement);
                    }
                    expanded.rich.push(message);
                }
                OpenedDeviceMessage::PushToken { timestamp, .. } => {
                    if !super::receive::valid_peer_timestamp(timestamp, current_unix_secs()) {
                        return Err(Error::InvalidStatement);
                    }
                    // No mobile push provider is registered by this Host. Only
                    // replay evidence survives; tokens never enter guest history.
                }
                OpenedDeviceMessage::DeviceControl(control) => {
                    // Historical lifecycle data is not fresh device authority.
                    // Validate it, but never resurrect/remove a live roster member.
                    if depth == 0
                        || !super::receive::valid_peer_timestamp(
                            control.timestamp,
                            current_unix_secs(),
                        )
                    {
                        return Err(Error::InvalidStatement);
                    }
                }
                OpenedDeviceMessage::CompactedHistory(reference) => {
                    if depth >= MAX_HISTORY_DEPTH
                        || !super::receive::valid_peer_timestamp(
                            reference.timestamp,
                            current_unix_secs(),
                        )
                    {
                        return Err(Error::InvalidStatement);
                    }
                    let digest = reference_digest(&reference);
                    if !seen.insert(digest) {
                        continue;
                    }
                    if existing_count + expanded.imports.len() >= MAX_HISTORY_IMPORTS {
                        return Err(Error::StorageUnavailable);
                    }
                    let rpc =
                        SessionHopRpc::connect(context, &self.product, &reference.endpoint).await?;
                    let ticket = FileTicket::from_bytes(&*reference.ticket)
                        .map_err(|_| Error::InvalidStatement)?;
                    let result = HopClient::new(&rpc)
                        .claim_compaction(reference.identifier, &ticket)
                        .await;
                    context.require_current()?;
                    let claimed = result.map_err(claim_error)?;
                    let mut nested = Vec::new();
                    for bytes in claimed.batch.messages() {
                        bytes_seen = bytes_seen
                            .checked_add(bytes.len())
                            .ok_or(Error::InvalidStatement)?;
                        if bytes_seen > MAX_EXPANDED_BYTES {
                            return Err(Error::StorageUnavailable);
                        }
                        let mut owned = Zeroizing::new(bytes.to_vec());
                        nested.push(
                            classify_message(&mut owned).map_err(|_| Error::InvalidStatement)?,
                        );
                    }
                    expanded.imports.push(HistoryImport {
                        peer,
                        digest,
                        pending: claimed.pending_ack.map(|ack| HistoryAck {
                            endpoint: reference.endpoint,
                            ticket: Secret32(*reference.ticket),
                            encoded: ack.encode(),
                        }),
                    });
                    work.extend(nested.into_iter().rev().map(|message| (message, depth + 1)));
                }
            }
        }
        Ok(expanded)
    }

    pub(super) async fn acknowledge_history(
        &self,
        context: &NativeChatContext,
    ) -> Result<(), Error> {
        let _gate = self.history_ack_gate.lock().await;
        let pending = self
            .store
            .read(|state| {
                state
                    .history_imports
                    .iter()
                    .filter(|entry| entry.pending.is_some())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .await?;
        for imported in pending {
            let Some(pending) = imported.pending else {
                continue;
            };
            let rpc = SessionHopRpc::connect(context, &self.product, &pending.endpoint).await?;
            let ticket =
                FileTicket::from_bytes(&pending.ticket.0).map_err(|_| Error::StorageUnavailable)?;
            let acknowledgment =
                PendingAck::restore(&pending.encoded).map_err(|_| Error::StorageUnavailable)?;
            HopClient::new(&rpc)
                .acknowledge(&acknowledgment, &ticket)
                .await
                .map_err(|_| Error::NetworkUnavailable)?;
            let valid = context.session_valid.clone();
            self.store
                .update(move |state| {
                    if !valid() {
                        return Err(Error::NotConnected);
                    }
                    if let Some(entry) = state.history_imports.iter_mut().find(|entry| {
                        entry.peer == imported.peer && entry.digest == imported.digest
                    }) {
                        entry.pending = None;
                    }
                    Ok(())
                })
                .await?;
        }
        Ok(())
    }
}
