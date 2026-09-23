// SPDX-License-Identifier: AGPL-3.0-only
//! One bounded, fenced transfer step; durable bytes and progress always precede ACK.

use super::*;
use crate::runtime::native_chat::{
    background::require_upload_authorized,
    hop::{
        DownloadProgress, HopClient, HopError, PreparedUpload, RootDescriptor,
        SenderProofProviding, UploadProgress,
    },
    hop_access::SessionHopRpc,
};
use std::sync::atomic::Ordering;

const PREPARED_CHUNK: u32 = 1 << 31;
const PREPARED_INLINE: u32 = u32::MAX;
const PREPARED_ROOT: u32 = u32::MAX - 1;
const MAX_BLOB_BYTES: usize = hop::HOP_CHUNK_BYTES + 128;

#[derive(Clone, Encode, Decode)]
struct PrivateBlob(Vec<u8>);
impl Drop for PrivateBlob {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

struct UploadSigner<'a> {
    context: &'a NativeChatContext,
    product: &'a str,
}
#[async_trait::async_trait]
impl SenderProofProviding for UploadSigner<'_> {
    async fn proof(&self, data_hash: &[u8; 32]) -> Result<hop::SenderProof, HopError> {
        require_upload_authorized(self.context, self.product)
            .await
            .map_err(|_| HopError::Transport("Chat upload authorization ended".into()))?;
        let signer = derive_sr25519_hard_path(
            &self.context.entropy,
            &["allowance", "bulletin", self.product],
        )
        .map_err(|_| HopError::Crypto)?;
        let submit_timestamp = current_unix_secs()
            .checked_mul(1000)
            .ok_or(HopError::InvalidProgress)?;
        let payload = hop::sender_proof_payload(data_hash, submit_timestamp);
        self.context
            .require_current()
            .map_err(|_| HopError::Transport("Chat session ended".into()))?;
        let signature = signer.sign_simple(b"substrate", &payload);
        Ok(hop::SenderProof {
            sender: hop::MultiSigner::Sr25519(signer.public.to_bytes()),
            signature: hop::MultiSignature::Sr25519(signature.to_bytes()),
            submit_timestamp,
        })
    }
}

fn remote_error(error: HopError) -> Error {
    match error {
        HopError::Crypto | HopError::Codec(_) | HopError::Integrity | HopError::InvalidProgress => {
            Error::InvalidStatement
        }
        HopError::Rpc { .. } | HopError::Transport(_) | HopError::NotFound(_) => {
            Error::NetworkUnavailable
        }
    }
}

impl NativeChatActor {
    async fn change_file<R: Send + 'static>(
        &self,
        context: &NativeChatContext,
        id: [u8; 32],
        change: impl FnOnce(&mut FileRecord) -> Result<R, Error> + Send + 'static,
    ) -> Result<R, Error> {
        context.require_current()?;
        let valid = context.session_valid.clone();
        self.store
            .update(move |state| {
                if !valid() {
                    return Err(Error::NotConnected);
                }
                let file = state
                    .files
                    .iter_mut()
                    .find(|file| file.id == id)
                    .ok_or(Error::OperationNotFound)?;
                change(file)
            })
            .await
    }

    async fn blob(
        &self,
        context: &NativeChatContext,
        id: [u8; 32],
        slot: u32,
        initial: impl FnOnce() -> Result<PrivateBlob, Error>,
    ) -> Result<Arc<ChatStateStore<PrivateBlob>>, Error> {
        ChatStateStore::open_file_chunk(context, &self.product, id, slot, initial).await
    }

    async fn write_chunk(
        &self,
        context: &NativeChatContext,
        id: [u8; 32],
        index: u32,
        mut bytes: Zeroizing<Vec<u8>>,
    ) -> Result<(), Error> {
        if index >= PREPARED_CHUNK || bytes.len() > hop::HOP_CHUNK_BYTES {
            return Err(Error::StorageUnavailable);
        }
        context.require_current()?;
        let expected = hash(&bytes);
        let stored = self
            .blob(context, id, index, move || {
                Ok(PrivateBlob(core::mem::take(&mut *bytes)))
            })
            .await?;
        stored
            .read(|blob| {
                if hash(&blob.0) == expected {
                    Ok(())
                } else {
                    Err(Error::OperationConflict)
                }
            })
            .await??;
        context.require_current()
    }

    pub(super) async fn read_chunk(
        &self,
        context: &NativeChatContext,
        id: [u8; 32],
        index: u32,
    ) -> Result<Zeroizing<Vec<u8>>, Error> {
        let stored = self
            .blob(context, id, index, || Err(Error::StorageUnavailable))
            .await?;
        context.require_current()?;
        stored
            .read(|blob| {
                if blob.0.len() > MAX_BLOB_BYTES {
                    return Err(Error::StorageUnavailable);
                }
                Ok(Zeroizing::new(blob.0.clone()))
            })
            .await?
    }

    pub(super) async fn read_source(
        &self,
        context: &NativeChatContext,
        file: &FileRecord,
        offset: u64,
        length: u32,
    ) -> Result<Zeroizing<Vec<u8>>, Error> {
        require_authorized(context, &self.product).await?;
        if length as usize > hop::HOP_CHUNK_BYTES
            || offset
                .checked_add(u64::from(length))
                .is_none_or(|end| end > u64::from(file.metadata.size_bytes))
        {
            return Err(Error::StorageUnavailable);
        }
        let source = file.source.as_ref().ok_or(Error::AttachmentsUnavailable)?;
        let bytes = Zeroizing::new(
            context
                .services
                .platform
                .read_chat_file(source.clone(), offset, length)
                .await
                .map_err(|_| Error::AttachmentsUnavailable)?,
        );
        context.require_current()?;
        if bytes.len() != length as usize {
            return Err(Error::AttachmentsUnavailable);
        }
        Ok(bytes)
    }

    #[allow(clippy::too_many_arguments)]
    async fn prepare_entry(
        &self,
        context: &NativeChatContext,
        file: &FileRecord,
        kind: PreparedKind,
        slot: u32,
        create: impl FnOnce() -> Result<PreparedUpload, HopError>,
        cache_chunks: u32,
        cache_bytes: u64,
    ) -> Result<(), Error> {
        // The immutable slot is created before the actor pointer. If that pointer
        // commit is lost, reopening the slot reuses the original nonce and hash.
        let stored = self
            .blob(context, file.id, slot, || {
                create()
                    .map(|prepared| PrivateBlob(prepared.encode()))
                    .map_err(remote_error)
            })
            .await?;
        let hash = stored
            .read(|blob| {
                PreparedUpload::restore(&blob.0)
                    .map(|prepared| prepared.hash())
                    .map_err(|_| Error::StorageUnavailable)
            })
            .await??;
        self.change_file(context, file.id, move |file| {
            file.prepared = Some(PreparedEntry { kind, slot, hash });
            file.cache_chunks = cache_chunks;
            file.cache_bytes = cache_bytes;
            file.recovering = false;
            Ok(())
        })
        .await
    }

    pub(super) async fn advance_file(
        &self,
        context: &NativeChatContext,
        id: [u8; 32],
    ) -> Result<bool, Error> {
        let _transfer = self.file_transfer_gate.lock().await;
        require_authorized(context, &self.product).await?;
        let file = self.file(id).await?;
        let ticket =
            FileTicket::from_bytes(&file.ticket.0).map_err(|_| Error::StorageUnavailable)?;
        if let Some(bytes) = &file.pending_ack {
            let ack = hop::PendingAck::restore(bytes).map_err(|_| Error::StorageUnavailable)?;
            let rpc = SessionHopRpc::connect(context, &self.product, &file.endpoint).await?;
            HopClient::new(&rpc)
                .acknowledge(&ack, &ticket)
                .await
                .map_err(remote_error)?;
            self.change_file(context, id, |file| {
                file.pending_ack = None;
                Ok(())
            })
            .await?;
            return Ok(true);
        }
        if file.ready {
            if let Some(source) = file.source {
                context.require_current()?;
                context
                    .services
                    .platform
                    .release_chat_file(source)
                    .await
                    .map_err(|_| Error::AttachmentsUnavailable)?;
                self.change_file(context, id, |file| {
                    file.source = None;
                    Ok(())
                })
                .await?;
                return Ok(true);
            }
            return Ok(false);
        }
        if file.incoming {
            // Forwarded/reused references may already have been ACKed elsewhere
            // in this actor. Reuse authenticated local bytes, not a deleted pool entry.
            let equivalent = self
                .store
                .read(|state| {
                    state
                        .files
                        .iter()
                        .find(|other| {
                            other.id != file.id
                                && other.ready
                                && other.root == file.root
                                && other.ticket.0 == file.ticket.0
                                && other.endpoint == file.endpoint
                                && other.metadata.size_bytes == file.metadata.size_bytes
                        })
                        .cloned()
                })
                .await?;
            if let Some(other) = equivalent {
                self.change_file(context, id, move |file| {
                    file.cache_id = other.cache_id;
                    file.cache_chunks = other.cache_chunks;
                    file.cache_bytes = other.cache_bytes;
                    file.descriptor = other.descriptor;
                    file.download = other.download;
                    file.ready = true;
                    file.recovering = false;
                    Ok(())
                })
                .await?;
                return Ok(true);
            }
            let rpc = SessionHopRpc::connect(context, &self.product, &file.endpoint).await?;
            let client = HopClient::new(&rpc);
            if let Some(bytes) = &file.descriptor {
                let root = RootDescriptor::restore(bytes).map_err(|_| Error::StorageUnavailable)?;
                let progress = DownloadProgress::restore(&file.download, &root)
                    .map_err(|_| Error::StorageUnavailable)?;
                let chunk = client
                    .claim_chunk(&root, progress, &ticket)
                    .await
                    .map_err(remote_error)?
                    .ok_or(Error::StorageUnavailable)?;
                self.write_chunk(context, file.cache_id, chunk.index, chunk.data)
                    .await?;
                let complete = chunk
                    .next_progress
                    .is_complete(&root)
                    .map_err(|_| Error::StorageUnavailable)?;
                self.change_file(context, id, move |file| {
                    file.cache_chunks = chunk.next_progress.next_chunk;
                    file.cache_bytes = chunk.next_progress.downloaded_bytes;
                    file.download = chunk.next_progress.encode();
                    file.pending_ack = chunk.pending_ack.map(|ack| ack.encode());
                    file.ready = complete;
                    file.recovering = false;
                    Ok(())
                })
                .await?;
            } else {
                let claimed = client
                    .claim_root(
                        file.root.ok_or(Error::StorageUnavailable)?,
                        &ticket,
                        Some(file.metadata.size_bytes),
                    )
                    .await
                    .map_err(remote_error)?;
                let inline = claimed.inline.is_some();
                if let Some(bytes) = claimed.inline {
                    self.write_chunk(context, file.cache_id, 0, bytes).await?;
                }
                self.change_file(context, id, move |file| {
                    file.descriptor = Some(claimed.descriptor.encode());
                    if inline {
                        file.cache_chunks = 1;
                        file.cache_bytes = u64::from(file.metadata.size_bytes);
                        file.ready = true;
                    } else {
                        file.download = DownloadProgress::default().encode();
                    }
                    file.pending_ack = claimed.pending_ack.map(|ack| ack.encode());
                    file.recovering = false;
                    Ok(())
                })
                .await?;
            }
            return Ok(true);
        }
        require_upload_authorized(context, &self.product).await?;
        if !self
            .store
            .read(|state| state.peer(&file.peer).is_ok_and(Peer::ready))
            .await?
        {
            return Err(Error::PeerNotReady);
        }
        if let Some(prepared) = file.prepared {
            let encoded = self.read_chunk(context, id, prepared.slot).await?;
            let upload =
                PreparedUpload::restore(&encoded).map_err(|_| Error::StorageUnavailable)?;
            if upload.hash() != prepared.hash {
                return Err(Error::StorageUnavailable);
            }
            let rpc = SessionHopRpc::connect(context, &self.product, &file.endpoint).await?;
            HopClient::new(&rpc)
                .submit(
                    &upload,
                    &UploadSigner {
                        context,
                        product: &self.product,
                    },
                )
                .await
                .map_err(remote_error)?;
            self.change_file(context, id, move |file| {
                match prepared.kind {
                    PreparedKind::Inline | PreparedKind::Root => {
                        file.root = Some(prepared.hash);
                        file.ready = true;
                    }
                    PreparedKind::Chunk { index, byte_len } => {
                        let mut progress = UploadProgress::restore(&file.upload)
                            .map_err(|_| Error::StorageUnavailable)?;
                        if progress.next_chunk().is_none_or(|next| next.index != index) {
                            return Err(Error::StorageUnavailable);
                        }
                        progress
                            .record_chunk(prepared.hash, byte_len as usize)
                            .map_err(|_| Error::StorageUnavailable)?;
                        file.upload = progress.encode();
                    }
                }
                file.prepared = None;
                file.recovering = false;
                Ok(())
            })
            .await?;
            return Ok(true);
        }
        if file.upload.is_empty() {
            let bytes = self
                .read_source(context, &file, 0, file.metadata.size_bytes)
                .await?;
            let stored = self
                .blob(context, id, PREPARED_INLINE, || {
                    PreparedUpload::inline(&bytes, &ticket)
                        .map(|upload| PrivateBlob(upload.encode()))
                        .map_err(remote_error)
                })
                .await?;
            self.write_chunk(context, file.cache_id, 0, bytes).await?;
            let hash = stored
                .read(|blob| {
                    PreparedUpload::restore(&blob.0)
                        .map(|upload| upload.hash())
                        .map_err(|_| Error::StorageUnavailable)
                })
                .await??;
            self.change_file(context, id, move |file| {
                file.prepared = Some(PreparedEntry {
                    kind: PreparedKind::Inline,
                    slot: PREPARED_INLINE,
                    hash,
                });
                file.cache_chunks = 1;
                file.cache_bytes = u64::from(file.metadata.size_bytes);
                Ok(())
            })
            .await?;
        } else {
            let progress =
                UploadProgress::restore(&file.upload).map_err(|_| Error::StorageUnavailable)?;
            if let Some(next) = progress.next_chunk() {
                let bytes = self
                    .read_source(context, &file, next.offset, next.byte_len as u32)
                    .await?;
                let slot = PREPARED_CHUNK | next.index;
                let stored = self
                    .blob(context, id, slot, || {
                        PreparedUpload::chunk(&bytes, &ticket)
                            .map(|upload| PrivateBlob(upload.encode()))
                            .map_err(remote_error)
                    })
                    .await?;
                self.write_chunk(context, file.cache_id, next.index, bytes)
                    .await?;
                let hash = stored
                    .read(|blob| {
                        PreparedUpload::restore(&blob.0)
                            .map(|upload| upload.hash())
                            .map_err(|_| Error::StorageUnavailable)
                    })
                    .await??;
                self.change_file(context, id, move |file| {
                    file.prepared = Some(PreparedEntry {
                        kind: PreparedKind::Chunk {
                            index: next.index,
                            byte_len: next.byte_len as u32,
                        },
                        slot,
                        hash,
                    });
                    file.cache_chunks = next.index + 1;
                    file.cache_bytes = next.offset + next.byte_len as u64;
                    Ok(())
                })
                .await?;
            } else {
                self.prepare_entry(
                    context,
                    &file,
                    PreparedKind::Root,
                    PREPARED_ROOT,
                    || progress.prepare_root(&ticket),
                    file.cache_chunks,
                    file.cache_bytes,
                )
                .await?;
            }
        }
        Ok(true)
    }

    pub(in crate::runtime::native_chat) async fn drive_files(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<bool, Error> {
        context.require_current()?;
        if self
            .store
            .read(|state| state.boundary.legacy_pending)
            .await?
        {
            // The migration view is derived from these retained file records.
            // Explicit progress resumes once the guest durably commits that view.
            return Ok(false);
        }
        require_authorized(context, &self.product).await?;
        let cursor = self.file_cursor.fetch_add(1, Ordering::Relaxed);
        let id = self
            .store
            .read(|state| {
                let count = state.files.len();
                (0..count)
                    .map(|offset| &state.files[(cursor % count + offset) % count])
                    .find(|file| file.pending())
                    .map(|file| file.id)
            })
            .await?;
        let mut progressed = false;
        if let Some(id) = id {
            match self.advance_file(context, id).await {
                Ok(changed) => progressed = changed,
                Err(error) => {
                    if context.require_current().is_err() {
                        return Err(Error::NotConnected);
                    }
                    if !self.file(id).await?.recovering {
                        self.change_file(context, id, |file| {
                            file.recovering = true;
                            Ok(())
                        })
                        .await?;
                    }
                    if matches!(error, Error::AccessNotGranted) {
                        return Err(error);
                    }
                }
            }
        }
        Ok(self.publish_ready_rich(context).await? || progressed)
    }

    async fn publish_ready_rich(
        self: &Arc<Self>,
        context: &NativeChatContext,
    ) -> Result<bool, Error> {
        let candidates = self
            .store
            .read(|state| {
                state
                    .rich_messages
                    .iter()
                    .filter(|message| {
                        !message.incoming
                            && !message.selecting
                            && state.peer(&message.peer).is_ok_and(Peer::ready)
                            && message.files.iter().all(|id| {
                                state.files.iter().any(|file| &file.id == id && file.ready)
                            })
                            && (!message.published
                                || state.outbox.iter().any(|outgoing| {
                                    outgoing.kind == OutgoingKind::Rich(message.key)
                                        && state.peer(&message.peer).is_ok_and(|peer| {
                                            peer.revision != outgoing.roster_revision
                                        })
                                }))
                    })
                    .map(|message| message.key)
                    .collect::<Vec<_>>()
            })
            .await?;
        let mut changed = false;
        for key in candidates {
            require_authorized(context, &self.product).await?;
            let actor = self.clone();
            let valid = context.session_valid.clone();
            self.store
                .update(move |state| {
                    if !valid() {
                        return Err(Error::NotConnected);
                    }
                    let record = state
                        .rich_messages
                        .iter()
                        .find(|message| message.key == key)
                        .cloned()
                        .ok_or(Error::StorageUnavailable)?;
                    let peer = state.peer(&record.peer)?.clone();
                    if !peer.ready() {
                        return Err(Error::PeerNotReady);
                    }
                    let attachments = record
                        .files
                        .iter()
                        .map(|id| {
                            let file = state
                                .files
                                .iter()
                                .find(|file| &file.id == id && file.ready)
                                .ok_or(Error::StorageUnavailable)?;
                            to_wire(file)
                        })
                        .collect::<Result<Vec<_>, Error>>()?;
                    let bytes = wire::encode_rich_text_message(
                        &record.message_id,
                        record.timestamp,
                        record.text.as_deref(),
                        Some(&attachments),
                    )
                    .map_err(|_| Error::InvalidRequest)?;
                    let messages = Zeroizing::new(vec![bytes]);
                    let encoded = Zeroizing::new(messages.encode());
                    let digest = hash(&encoded);
                    let kind = OutgoingKind::Rich(key);
                    let position = state.outbox.iter().position(|entry| entry.kind == kind);
                    if record.published && position.is_none() {
                        return Ok(());
                    }
                    if let Some(position) = position
                        && state.outbox[position].digest != digest
                    {
                        return Err(Error::OperationConflict);
                    }
                    let statement = actor.multi_statement(
                        state,
                        &peer,
                        &peer.active_devices(),
                        &record.request_id,
                        &messages,
                    )?;
                    if let Some(position) = position {
                        let outgoing = &mut state.outbox[position];
                        outgoing.statement = statement;
                        outgoing.roster_revision = peer.revision;
                        outgoing.last_attempt = 0;
                    } else {
                        state.queue(Outgoing {
                            peer: record.peer,
                            request_id: record.request_id,
                            digest,
                            kind,
                            roster_revision: peer.revision,
                            statement,
                            last_attempt: 0,
                        })?;
                    }
                    state
                        .rich_messages
                        .iter_mut()
                        .find(|message| message.key == key)
                        .ok_or(Error::StorageUnavailable)?
                        .published = true;
                    Ok(())
                })
                .await?;
            changed = true;
        }
        Ok(changed)
    }
}

fn to_wire(file: &FileRecord) -> Result<wire::V2FileVariant, Error> {
    let general = wire::V2GeneralFileMeta {
        mime_type: file.metadata.mime_type.clone(),
        file_size: file.metadata.size_bytes,
    };
    let meta = match &file.metadata.kind {
        HostNativeChatAttachmentKind::File => wire::V2FileMeta::General(general),
        HostNativeChatAttachmentKind::Image {
            width,
            height,
            thumbnail,
        } => wire::V2FileMeta::Image(wire::V2ImageFileMeta {
            general,
            width: *width,
            height: *height,
            thumbnail: thumbnail.clone(),
        }),
        HostNativeChatAttachmentKind::Video {
            duration_seconds,
            thumbnail,
        } => wire::V2FileMeta::Video(wire::V2VideoFileMeta {
            general,
            duration: *duration_seconds,
            thumbnail: thumbnail.clone(),
        }),
    };
    Ok(wire::V2FileVariant::P2pMixnet(wire::V2P2pMixnetFile {
        identifier: file.root.ok_or(Error::StorageUnavailable)?.to_vec(),
        claim_ticket: file.ticket.0.to_vec(),
        node: wire::V2NodeEndpoint::WssUrl(file.endpoint.clone()),
        meta,
    }))
}
