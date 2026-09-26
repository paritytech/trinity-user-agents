// SPDX-License-Identifier: AGPL-3.0-only
//! Expand authenticated HOP history privately; custody commits precede every ACK.

use std::collections::BTreeSet;

use super::*;
use crate::runtime::chat_device::{CompactedHistory, OpenedDeviceMessage, classify_message};
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

/// A bounded handoff, not a conversation: immutable authenticated HOP pages
/// survive retries until the product prepares the real native acknowledgment.
#[derive(Clone, Encode, Decode)]
pub(super) struct HistoryDelivery {
    id: [u8; 32],
    peer: [u8; 32],
    sender: [u8; 32],
    route: HostNativeChatRoute,
    request_id: String,
    pages: Vec<SecretPage>,
    imports: Vec<HistoryImport>,
}

#[derive(Clone, Encode, Decode)]
struct SecretPage(Vec<u8>);
impl Drop for SecretPage {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

// The byte bound, not an incidental page count, limits authenticated history.
// Tiny native messages can legitimately require many 256-message pages.
const MAX_OPEN_PAGES: usize = MAX_EXPANDED_BYTES / 256 + 1;
const MAX_PAGE_BYTES: usize = 256 * 1024;
const MAX_PENDING_OPENS: usize = 16;

pub(super) fn validate_deliveries(deliveries: &[HistoryDelivery]) -> Result<(), Error> {
    if deliveries.len() > MAX_PENDING_OPENS {
        return Err(Error::StorageUnavailable);
    }
    let mut ids = BTreeSet::new();
    let mut total = 0usize;
    for delivery in deliveries {
        if !ids.insert(delivery.id)
            || delivery.pages.is_empty()
            || delivery.pages.len() > MAX_OPEN_PAGES
        {
            return Err(Error::StorageUnavailable);
        }
        valid_id(&delivery.request_id).map_err(|_| Error::StorageUnavailable)?;
        validate_imports(&delivery.imports)?;
        for page in &delivery.pages {
            total = total
                .checked_add(page.0.len())
                .ok_or(Error::StorageUnavailable)?;
            if page.0.len() > MAX_PAGE_BYTES || total > MAX_EXPANDED_BYTES {
                return Err(Error::StorageUnavailable);
            }
        }
    }
    Ok(())
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
    pub(super) async fn open_history(
        self: &Arc<Self>,
        context: &NativeChatContext,
        peer: [u8; 32],
        sender: [u8; 32],
        route: HostNativeChatRoute,
        request_id: String,
        mut plaintext: Zeroizing<Vec<u8>>,
    ) -> Result<(Vec<HostNativeChatOpened>, Option<HostNativeChatOpenPage>), Error> {
        let id = hash(
            &(
                b"native-chat-open-v3",
                peer,
                sender,
                route,
                &request_id,
                hash(&plaintext),
            )
                .encode(),
        );
        if self
            .store
            .read(|state| state.boundary.history.iter().any(|entry| entry.id == id))
            .await?
        {
            return self.continue_open(context, id, 0).await;
        }
        let wire::V2StatementTransportData::Request { messages, .. } =
            wire::decode_transport_plaintext(&plaintext).map_err(|_| Error::InvalidStatement)?
        else {
            return Err(Error::InvalidStatement);
        };
        let mut raw = Zeroizing::new(messages);
        let mut work: Vec<_> = core::mem::take(&mut *raw)
            .into_iter()
            .rev()
            .map(|bytes| (Zeroizing::new(bytes), 0usize))
            .collect();
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
        let mut expanded = Zeroizing::new(Vec::<Vec<u8>>::new());
        let mut imports = Vec::new();
        let mut rich = Vec::new();
        let mut bytes_seen = 0usize;
        let mut had_history = false;
        // Profile references are the Host's, not the product's: collected here,
        // stored after the open commits, and cut out of what the product sees.
        let mut profile_references = Vec::new();
        let mut stripped = false;
        while let Some((mut bytes, depth)) = work.pop() {
            context.require_current()?;
            bytes_seen = bytes_seen
                .checked_add(bytes.len())
                .ok_or(Error::InvalidStatement)?;
            if bytes_seen > MAX_EXPANDED_BYTES {
                return Err(Error::StorageUnavailable);
            }
            let message = classify_message(&mut bytes).map_err(|_| Error::InvalidStatement)?;
            match message {
                OpenedDeviceMessage::CompactedHistory(reference) => {
                    had_history = true;
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
                    if existing_count + imports.len() >= MAX_HISTORY_IMPORTS {
                        return Err(Error::StorageUnavailable);
                    }
                    let rpc =
                        SessionHopRpc::connect(context, &self.product, &reference.endpoint).await?;
                    let ticket = FileTicket::from_bytes(&*reference.ticket)
                        .map_err(|_| Error::InvalidStatement)?;
                    let claimed = HopClient::new(&rpc)
                        .claim_compaction(reference.identifier, &ticket)
                        .await
                        .map_err(claim_error)?;
                    context.require_current()?;
                    let mut nested = Vec::new();
                    for frame in claimed.batch.messages() {
                        if frame.len() > MAX_PAGE_BYTES || nested.len() >= 65_536 {
                            return Err(Error::StorageUnavailable);
                        }
                        nested.push((Zeroizing::new(frame.to_vec()), depth + 1));
                    }
                    imports.push(HistoryImport {
                        peer,
                        digest,
                        pending: claimed.pending_ack.map(|ack| HistoryAck {
                            endpoint: reference.endpoint,
                            ticket: Secret32(*reference.ticket),
                            encoded: ack.encode(),
                        }),
                    });
                    work.extend(nested.into_iter().rev());
                }
                OpenedDeviceMessage::RichContent(message) => {
                    if !super::receive::valid_peer_timestamp(message.timestamp, current_unix_secs())
                    {
                        return Err(Error::InvalidStatement);
                    }
                    rich.push(message);
                    expanded.push(core::mem::take(&mut *bytes));
                }
                OpenedDeviceMessage::Payment(memo) => {
                    if !super::receive::valid_peer_timestamp(memo.timestamp, current_unix_secs()) {
                        return Err(Error::InvalidStatement);
                    }
                    expanded.push(core::mem::take(&mut *bytes));
                }
                OpenedDeviceMessage::Ordinary(frame) => {
                    let message =
                        wire::decode_message(&frame).map_err(|_| Error::InvalidStatement)?;
                    if !super::receive::valid_peer_timestamp(message.timestamp, current_unix_secs())
                    {
                        return Err(Error::InvalidStatement);
                    }
                    expanded.push(frame);
                }
                OpenedDeviceMessage::DeviceControl(control) => {
                    if !super::receive::valid_peer_timestamp(control.timestamp, current_unix_secs())
                    {
                        return Err(Error::InvalidStatement);
                    }
                    // Compacted lifecycle events never restore live authority.
                    if depth == 0 {
                        expanded.push(core::mem::take(&mut *bytes));
                    }
                }
                OpenedDeviceMessage::PushToken { timestamp, .. } => {
                    if !super::receive::valid_peer_timestamp(timestamp, current_unix_secs()) {
                        return Err(Error::InvalidStatement);
                    }
                    if depth == 0 {
                        expanded.push(core::mem::take(&mut *bytes));
                    }
                }
                OpenedDeviceMessage::ProfileReference(frame) => {
                    if !super::receive::valid_peer_timestamp(frame.timestamp, current_unix_secs()) {
                        return Err(Error::InvalidStatement);
                    }
                    // Never forwarded, whatever the depth; only a live frame
                    // updates what this Host holds, never compacted history.
                    stripped = true;
                    if depth == 0 {
                        profile_references.push(frame);
                    }
                }
            }
        }
        let rich = self.prepare_rich(context, peer, &request_id, rich).await?;
        let valid = context.session_valid.clone();
        if imports.is_empty() {
            self.store
                .update(move |state| {
                    if !valid() {
                        return Err(Error::NotConnected);
                    }
                    files::merge_received(state, rich)
                })
                .await?;
            self.record_profile_references(context, peer, profile_references)
                .await?;
            // Preserve the original canonical request when no HOP expansion was
            // needed, except references already transferred by legacy migration.
            let plaintext = if had_history || stripped {
                wire::encode_transport_request_plaintext(&request_id, &expanded)
                    .map_err(|_| Error::InvalidStatement)?
            } else {
                core::mem::take(&mut *plaintext)
            };
            return Ok((
                vec![HostNativeChatOpened {
                    peer_identity: peer,
                    sender_account_id: sender,
                    route,
                    plaintext,
                }],
                None,
            ));
        }
        let mut pages = Vec::new();
        let mut batch = Zeroizing::new(Vec::<Vec<u8>>::new());
        let mut batch_bytes = 1 + request_id.encoded_size() + 5;
        for frame in expanded.iter_mut() {
            let mut frame = Zeroizing::new(core::mem::take(frame));
            let size = frame.encoded_size();
            if size + 1 + request_id.encoded_size() + 5 > MAX_PAGE_BYTES {
                return Err(Error::StorageUnavailable);
            }
            if batch.len() == 256 || batch_bytes + size > MAX_PAGE_BYTES {
                pages.push(SecretPage(
                    wire::encode_transport_request_plaintext(&request_id, &batch)
                        .map_err(|_| Error::InvalidStatement)?,
                ));
                for bytes in batch.iter_mut() {
                    bytes.zeroize();
                }
                batch.clear();
                batch_bytes = 1 + request_id.encoded_size() + 5;
            }
            batch_bytes += size;
            batch.push(core::mem::take(&mut *frame));
        }
        if !batch.is_empty() || pages.is_empty() {
            pages.push(SecretPage(
                wire::encode_transport_request_plaintext(&request_id, &batch)
                    .map_err(|_| Error::InvalidStatement)?,
            ));
        }
        let delivery = HistoryDelivery {
            id,
            peer,
            sender,
            route,
            request_id,
            pages,
            imports,
        };
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                if state.boundary.history.len() >= MAX_PENDING_OPENS {
                    return Err(Error::StorageUnavailable);
                }
                files::merge_received(state, rich)?;
                state.boundary.history.push(delivery);
                validate_deliveries(&state.boundary.history)
            })
            .await?;
        self.record_profile_references(context, peer, profile_references)
            .await?;
        self.continue_open(context, id, 0).await
    }

    /// Keep the newest profile reference each frame carries for `peer`, in
    /// this product's received-reference slot. `None` withdraws it.
    async fn record_profile_references(
        &self,
        context: &NativeChatContext,
        peer: [u8; 32],
        frames: Vec<crate::runtime::chat_device::ProfileReferenceFrame>,
    ) -> Result<(), Error> {
        for frame in frames {
            crate::runtime::profile::record_received_reference(
                &*context.services.platform,
                &self.product,
                peer,
                frame.discloser_product_id,
                frame.reference,
            )
            .await
            .map_err(|_| Error::StorageUnavailable)?;
        }
        Ok(())
    }

    pub(in crate::runtime::native_chat) async fn continue_open(
        self: &Arc<Self>,
        context: &NativeChatContext,
        open_id: [u8; 32],
        cursor: u32,
    ) -> Result<(Vec<HostNativeChatOpened>, Option<HostNativeChatOpenPage>), Error> {
        context.require_current()?;
        self.require_migrated().await?;
        self.store
            .read(|state| {
                let delivery = state
                    .boundary
                    .history
                    .iter()
                    .find(|entry| entry.id == open_id)
                    .ok_or(Error::OperationNotFound)?;
                let page = delivery
                    .pages
                    .get(cursor as usize)
                    .ok_or(Error::InvalidRequest)?;
                Ok((
                    vec![HostNativeChatOpened {
                        peer_identity: delivery.peer,
                        sender_account_id: delivery.sender,
                        route: delivery.route,
                        plaintext: page.0.clone(),
                    }],
                    Some(HostNativeChatOpenPage {
                        open_id,
                        cursor,
                        next_cursor: ((cursor as usize) + 1 < delivery.pages.len())
                            .then_some(cursor + 1),
                    }),
                ))
            })
            .await?
    }

    pub(super) async fn commit_history(
        &self,
        context: &NativeChatContext,
        peer: [u8; 32],
        request_id: &str,
    ) -> Result<(), Error> {
        let valid = context.session_valid.clone();
        let request_id = request_id.to_owned();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                for delivery in state
                    .boundary
                    .history
                    .iter()
                    .filter(|entry| entry.peer == peer && entry.request_id == request_id)
                {
                    for imported in &delivery.imports {
                        if !state.history_imports.iter().any(|entry| {
                            entry.peer == imported.peer && entry.digest == imported.digest
                        }) {
                            state.history_imports.push(imported.clone());
                        }
                    }
                }
                validate_imports(&state.history_imports)?;
                state
                    .boundary
                    .history
                    .retain(|entry| !(entry.peer == peer && entry.request_id == request_id));
                Ok(())
            })
            .await?;
        self.acknowledge_history(context).await
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
