// SPDX-License-Identifier: AGPL-3.0-only
//! Private file capabilities, immutable send intents and safe rich projections.

mod transfer;

use super::*;
use crate::runtime::chat_device::{MAX_ATTACHMENTS, RichContent, validate_metadata};
use crate::runtime::native_chat::{
    background::require_authorized,
    hop::{self, FileTicket},
};
use truapi_platform::{NativeChatFileExportRequest, NativeChatFilePickRequest};

const MAX_FILES: usize = 4096;
const MAX_RICH_MESSAGES: usize = 4096;
const MAX_PICKED_FILES: u32 = 16;

#[derive(Clone, Encode, Decode)]
pub(super) enum PreparedKind {
    Inline,
    Chunk { index: u32, byte_len: u32 },
    Root,
}

#[derive(Clone, Encode, Decode)]
pub(super) struct PreparedEntry {
    kind: PreparedKind,
    slot: u32,
    hash: [u8; 32],
}

#[derive(Clone, Encode, Decode)]
pub(super) struct FileRecord {
    id: [u8; 32],
    cache_id: [u8; 32],
    peer: [u8; 32],
    incoming: bool,
    metadata: HostNativeChatAttachmentMetadata,
    ticket: Secret32,
    endpoint: String,
    root: Option<[u8; 32]>,
    source: Option<String>,
    upload: Vec<u8>,
    prepared: Option<PreparedEntry>,
    descriptor: Option<Vec<u8>>,
    download: Vec<u8>,
    cache_chunks: u32,
    cache_bytes: u64,
    pending_ack: Option<Vec<u8>>,
    ready: bool,
    recovering: bool,
}

impl FileRecord {
    fn pending(&self) -> bool {
        !self.ready || self.pending_ack.is_some() || self.source.is_some()
    }

    fn view(&self) -> Result<HostNativeChatAttachment, Error> {
        let state = if self.ready {
            HostNativeChatAttachmentState::Ready
        } else if self.recovering {
            HostNativeChatAttachmentState::Recovering
        } else if self.incoming {
            HostNativeChatAttachmentState::Downloading {
                downloaded_bytes: self.cache_bytes as u32,
            }
        } else {
            let uploaded_bytes = if self.upload.is_empty() {
                0
            } else {
                hop::UploadProgress::restore(&self.upload)
                    .map(|progress| progress.uploaded_size() as u32)
                    .map_err(|_| Error::StorageUnavailable)?
            };
            HostNativeChatAttachmentState::Uploading { uploaded_bytes }
        };
        Ok(HostNativeChatAttachment {
            attachment_id: self.id,
            metadata: self.metadata.clone(),
            state,
        })
    }
}

#[derive(Clone, Encode, Decode)]
pub(super) struct RichRecord {
    key: [u8; 32],
    peer: [u8; 32],
    incoming: bool,
    client_request_id: Option<String>,
    request_id: String,
    message_id: String,
    timestamp: u64,
    kind: HostNativeChatRichMessageKind,
    text: Option<String>,
    files: Vec<[u8; 32]>,
    digest: [u8; 32],
    selecting: bool,
    published: bool,
}

pub(super) struct IncomingRich {
    messages: Vec<RichRecord>,
    files: Vec<FileRecord>,
}

pub(super) fn validate(state: &State) -> Result<(), Error> {
    if state.files.len() > MAX_FILES || state.rich_messages.len() > MAX_RICH_MESSAGES {
        return Err(Error::StorageUnavailable);
    }
    let mut ids = std::collections::BTreeSet::new();
    for file in &state.files {
        if file.id == [0; 32]
            || !ids.insert(file.id)
            || file.cache_bytes > u64::from(file.metadata.size_bytes)
            || (file.ready
                && (file.root.is_none() || file.cache_bytes != u64::from(file.metadata.size_bytes)))
            || (file.incoming && (file.source.is_some() || file.root.is_none()))
        {
            return Err(Error::StorageUnavailable);
        }
        validate_metadata(&file.metadata).map_err(|_| Error::StorageUnavailable)?;
        if !file.endpoint.starts_with("wss://") || file.endpoint.len() > 4096 {
            return Err(Error::StorageUnavailable);
        }
        if !file.upload.is_empty() {
            let progress = hop::UploadProgress::restore(&file.upload)
                .map_err(|_| Error::StorageUnavailable)?;
            if progress.total_size() != file.metadata.size_bytes {
                return Err(Error::StorageUnavailable);
            }
        }
        if let Some(bytes) = &file.descriptor {
            let root =
                hop::RootDescriptor::restore(bytes).map_err(|_| Error::StorageUnavailable)?;
            if Some(root.entry_hash()) != file.root
                || root.total_size() != u64::from(file.metadata.size_bytes)
            {
                return Err(Error::StorageUnavailable);
            }
            if let hop::RootDescriptor::Chunked { .. } = root {
                let progress = hop::DownloadProgress::restore(&file.download, &root)
                    .map_err(|_| Error::StorageUnavailable)?;
                if progress.next_chunk != file.cache_chunks
                    || progress.downloaded_bytes != file.cache_bytes
                {
                    return Err(Error::StorageUnavailable);
                }
            }
        }
        if let Some(ack) = &file.pending_ack {
            hop::PendingAck::restore(ack).map_err(|_| Error::StorageUnavailable)?;
        }
    }
    let mut messages = std::collections::BTreeSet::new();
    for message in &state.rich_messages {
        if !messages.insert(message.key)
            || message.files.len() > MAX_ATTACHMENTS
            || message.files.iter().any(|id| !ids.contains(id))
            || (!message.selecting && message.files.is_empty())
        {
            return Err(Error::StorageUnavailable);
        }
        let distinct: std::collections::BTreeSet<_> = message.files.iter().collect();
        if distinct.len() != message.files.len() {
            return Err(Error::StorageUnavailable);
        }
    }
    Ok(())
}

pub(super) fn public_views(state: &State) -> Result<Vec<HostNativeChatRichMessage>, Error> {
    state
        .rich_messages
        .iter()
        .filter(|message| !message.selecting)
        .map(|message| {
            let attachments = message
                .files
                .iter()
                .map(|id| {
                    state
                        .files
                        .iter()
                        .find(|file| &file.id == id)
                        .ok_or(Error::StorageUnavailable)?
                        .view()
                })
                .collect::<Result<Vec<_>, _>>()?;
            Ok(HostNativeChatRichMessage {
                peer_identity: message.peer,
                incoming: message.incoming,
                request_id: message.request_id.clone(),
                message_id: message.message_id.clone(),
                timestamp: message.timestamp,
                kind: message.kind.clone(),
                text: message.text.clone(),
                attachments,
            })
        })
        .collect()
}

impl NativeChatActor {
    pub(super) async fn prepare_rich(
        &self,
        context: &NativeChatContext,
        peer: [u8; 32],
        request_id: &str,
        messages: Vec<RichContent>,
    ) -> Result<IncomingRich, Error> {
        let mut prepared = IncomingRich {
            messages: Vec::new(),
            files: Vec::new(),
        };
        if messages.is_empty() {
            return Ok(prepared);
        }
        require_authorized(context, &self.product).await?;
        let allowed = context
            .services
            .platform
            .allowed_hop_endpoints(context.services.bulletin.genesis_hash())
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        context.require_current()?;
        for message in messages {
            let key = hash(&(peer, true, &message.message_id).encode());
            let mut file_ids = Vec::with_capacity(message.files.len());
            for reference in message.files {
                truapi_platform::ensure_allowed_hop_endpoint(&reference.endpoint, &allowed)
                    .map_err(|_| Error::InvalidStatement)?;
                let binding = Zeroizing::new(
                    (
                        self.public.identity_account_id,
                        peer,
                        reference.identifier,
                        &*reference.ticket,
                        &reference.endpoint,
                        &reference.metadata,
                    )
                        .encode(),
                );
                let id = hash(&binding);
                if id == [0; 32] || file_ids.contains(&id) {
                    return Err(Error::InvalidStatement);
                }
                file_ids.push(id);
                prepared.files.push(FileRecord {
                    id,
                    cache_id: id,
                    peer,
                    incoming: true,
                    metadata: reference.metadata,
                    ticket: Secret32(*reference.ticket),
                    endpoint: reference.endpoint,
                    root: Some(reference.identifier),
                    source: None,
                    upload: Vec::new(),
                    prepared: None,
                    descriptor: None,
                    download: Vec::new(),
                    cache_chunks: 0,
                    cache_bytes: 0,
                    pending_ack: None,
                    ready: false,
                    recovering: false,
                });
            }
            prepared.messages.push(RichRecord {
                key,
                peer,
                incoming: true,
                client_request_id: None,
                request_id: request_id.to_owned(),
                message_id: message.message_id,
                timestamp: message.timestamp,
                kind: message.kind,
                text: message.text,
                files: file_ids,
                digest: message.digest,
                selecting: false,
                published: true,
            });
        }
        Ok(prepared)
    }

    pub(in crate::runtime::native_chat) async fn prepare_attachments(
        self: &Arc<Self>,
        context: &NativeChatContext,
        peer: [u8; 32],
        request_id: String,
        text: Option<String>,
    ) -> Result<(), Error> {
        self.require_migrated().await?;
        let _selection = self.file_selection_gate.lock().await;
        require_authorized(context, &self.product).await?;
        if request_id.is_empty()
            || request_id.len() > 128
            || text.as_ref().is_some_and(|text| text.len() > 8192)
        {
            return Err(Error::InvalidRequest);
        }
        let key = hash(&(self.public.account_id, &self.product, &request_id).encode());
        let existing = self
            .store
            .read(|state| {
                state
                    .rich_messages
                    .iter()
                    .find(|message| message.key == key)
                    .cloned()
            })
            .await?;
        if let Some(existing) = &existing {
            if existing.incoming
                || existing.peer != peer
                || existing.text != text
                || existing.client_request_id.as_ref() != Some(&request_id)
            {
                return Err(Error::OperationConflict);
            }
            if !existing.selecting {
                return Ok(());
            }
        }
        let recipient = self
            .store
            .read(|state| state.peer(&peer).cloned())
            .await??;
        if !recipient.ready() {
            return Err(Error::PeerNotReady);
        }
        if existing.is_none() {
            let valid = context.session_valid.clone();
            let text = text.clone();
            let request_id = request_id.clone();
            self.store
                .update(move |state| {
                    if !valid() {
                        return Err(Error::NotConnected);
                    }
                    if state.rich_messages.len() >= MAX_RICH_MESSAGES {
                        return Err(Error::StorageUnavailable);
                    }
                    state.rich_messages.push(RichRecord {
                        key,
                        peer,
                        incoming: false,
                        client_request_id: Some(request_id),
                        request_id: format!("file-{}", hex::encode(key)),
                        message_id: format!("attachment-{}", hex::encode(key)),
                        timestamp: current_unix_secs().saturating_mul(1000),
                        kind: HostNativeChatRichMessageKind::Message,
                        text,
                        files: Vec::new(),
                        digest: key,
                        selecting: true,
                        published: false,
                    });
                    Ok(())
                })
                .await?;
        }
        let allowed = context
            .services
            .platform
            .allowed_hop_endpoints(context.services.bulletin.genesis_hash())
            .await
            .map_err(|_| Error::NetworkUnavailable)?;
        let endpoint = allowed
            .iter()
            .find(|endpoint| {
                truapi_platform::ensure_allowed_hop_endpoint(endpoint, &allowed).is_ok()
            })
            .cloned()
            .ok_or(Error::AttachmentsUnavailable)?;
        require_authorized(context, &self.product).await?;
        let selected = context
            .services
            .platform
            .pick_chat_files(NativeChatFilePickRequest {
                product_id: self.product.clone(),
                peer_identity: peer,
                peer_username: recipient.username,
                max_files: MAX_PICKED_FILES,
            })
            .await
            .map_err(|_| Error::AttachmentsUnavailable)?;
        let valid_selection = context.require_current().and_then(|_| {
            if selected.is_empty() {
                return Err(Error::UserRejected);
            }
            if selected.len() > MAX_PICKED_FILES as usize {
                return Err(Error::InvalidRequest);
            }
            let mut sources = std::collections::BTreeSet::new();
            for file in &selected {
                if !sources.insert(&file.source_id) {
                    return Err(Error::InvalidRequest);
                }
                if file.source_id.is_empty()
                    || file.source_id.len() > 1024
                    || file.source_id.chars().any(char::is_control)
                {
                    return Err(Error::InvalidRequest);
                }
                validate_metadata(&file.metadata).map_err(|_| Error::InvalidRequest)?;
            }
            Ok(())
        });
        if let Err(error) = valid_selection {
            for file in selected {
                let _ = context
                    .services
                    .platform
                    .release_chat_file(file.source_id)
                    .await;
            }
            return Err(error);
        }
        let prepared = selected
            .iter()
            .map(|selected| {
                let id = random_bytes()?;
                if id == [0; 32] {
                    return Err(Error::StorageUnavailable);
                }
                let ticket = FileTicket::generate().map_err(|_| Error::StorageUnavailable)?;
                let upload = if selected.metadata.size_bytes as usize > hop::HOP_INLINE_MAX_BYTES {
                    hop::UploadProgress::new(selected.metadata.size_bytes)
                        .map_err(|_| Error::InvalidRequest)?
                        .encode()
                } else {
                    Vec::new()
                };
                Ok(FileRecord {
                    id,
                    cache_id: id,
                    peer,
                    incoming: false,
                    metadata: selected.metadata.clone(),
                    ticket: Secret32(*ticket.as_bytes()),
                    endpoint: endpoint.clone(),
                    root: None,
                    source: Some(selected.source_id.clone()),
                    upload,
                    prepared: None,
                    descriptor: None,
                    download: Vec::new(),
                    cache_chunks: 0,
                    cache_bytes: 0,
                    pending_ack: None,
                    ready: false,
                    recovering: false,
                })
            })
            .collect::<Result<Vec<_>, Error>>();
        let files = match prepared {
            Ok(files) => files,
            Err(error) => {
                for file in selected {
                    let _ = context
                        .services
                        .platform
                        .release_chat_file(file.source_id)
                        .await;
                }
                return Err(error);
            }
        };
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                if state.files.len() + files.len() > MAX_FILES {
                    return Err(Error::StorageUnavailable);
                }
                let record = state
                    .rich_messages
                    .iter_mut()
                    .find(|record| record.key == key)
                    .ok_or(Error::OperationNotFound)?;
                if !record.selecting {
                    return Err(Error::OperationConflict);
                }
                record.files = files.iter().map(|file| file.id).collect();
                record.selecting = false;
                state.files.extend(files);
                Ok(())
            })
            .await?;
        Ok(())
    }

    pub(in crate::runtime::native_chat) async fn open_attachment(
        self: &Arc<Self>,
        context: &NativeChatContext,
        id: [u8; 32],
    ) -> Result<(), Error> {
        self.require_migrated().await?;
        let _export = self.file_export_gate.lock().await;
        require_authorized(context, &self.product).await?;
        let file = self.file(id).await?;
        let username = self
            .store
            .read(|state| {
                state
                    .peer(&file.peer)
                    .ok()
                    .and_then(|peer| peer.username.clone())
            })
            .await?;
        let handle = context
            .services
            .platform
            .begin_chat_file_export(NativeChatFileExportRequest {
                product_id: self.product.clone(),
                peer_identity: file.peer,
                peer_username: username,
                metadata: file.metadata.clone(),
            })
            .await
            .map_err(|_| Error::AttachmentsUnavailable)?
            .ok_or(Error::UserRejected)?;
        let result = async {
            loop {
                require_authorized(context, &self.product).await?;
                let current = self.file(id).await?;
                if current.ready || !current.incoming {
                    break;
                }
                self.advance_file(context, id).await?;
            }
            // Keep source release and chunk progression from racing an export.
            let _transfer = self.file_transfer_gate.lock().await;
            let current = self.file(id).await?;
            let mut offset = 0u64;
            let mut index = 0u32;
            while offset < u64::from(current.metadata.size_bytes) {
                require_authorized(context, &self.product).await?;
                let mut bytes = if current.ready || index < current.cache_chunks {
                    self.read_chunk(context, current.cache_id, index).await?
                } else {
                    self.read_source(
                        context,
                        &current,
                        offset,
                        (u64::from(current.metadata.size_bytes) - offset)
                            .min(hop::HOP_CHUNK_BYTES as u64) as u32,
                    )
                    .await?
                };
                if bytes.is_empty()
                    || bytes.len() > hop::HOP_CHUNK_BYTES
                    || offset + bytes.len() as u64 > u64::from(current.metadata.size_bytes)
                {
                    return Err(Error::StorageUnavailable);
                }
                require_authorized(context, &self.product).await?;
                let length = bytes.len() as u64;
                context
                    .services
                    .platform
                    .write_chat_file_export(handle.clone(), offset, core::mem::take(&mut *bytes))
                    .await
                    .map_err(|_| Error::AttachmentsUnavailable)?;
                context.require_current()?;
                offset += length;
                index += 1;
            }
            require_authorized(context, &self.product).await?;
            context
                .services
                .platform
                .finish_chat_file_export(handle.clone())
                .await
                .map_err(|_| Error::AttachmentsUnavailable)?;
            context.require_current()
        }
        .await;
        if result.is_err() {
            let _ = context
                .services
                .platform
                .cancel_chat_file_export(handle)
                .await;
        }
        result
    }

    async fn file(&self, id: [u8; 32]) -> Result<FileRecord, Error> {
        self.store
            .read(|state| {
                state
                    .files
                    .iter()
                    .find(|file| file.id == id)
                    .cloned()
                    .ok_or(Error::OperationNotFound)
            })
            .await?
    }
}

pub(super) fn merge_received(state: &mut State, prepared: IncomingRich) -> Result<(), Error> {
    for file in prepared.files {
        if let Some(existing) = state.files.iter().find(|existing| existing.id == file.id) {
            if existing.peer != file.peer
                || existing.metadata != file.metadata
                || existing.root != file.root
                || existing.ticket.0 != file.ticket.0
                || existing.endpoint != file.endpoint
            {
                return Err(Error::InvalidStatement);
            }
        } else {
            if state.files.len() >= MAX_FILES {
                return Err(Error::StorageUnavailable);
            }
            state.files.push(file);
        }
    }
    for message in prepared.messages {
        if let Some(existing) = state
            .rich_messages
            .iter()
            .find(|existing| existing.key == message.key)
        {
            if existing.digest != message.digest {
                return Err(Error::InvalidStatement);
            }
        } else {
            if state.rich_messages.len() >= MAX_RICH_MESSAGES {
                return Err(Error::StorageUnavailable);
            }
            state.rich_messages.push(message);
        }
    }
    Ok(())
}
