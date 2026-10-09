// SPDX-License-Identifier: AGPL-3.0-only
//! Host-private native HOP protocol. Adapted from brevity-chat/src/hop.rs,
//! brevity-dozer/core d504259b60b88ca42f70a8378186a714887ef19f.
//!
//! Wire indices, ticket KDFs, proof domains, CryptoKit combined ciphertexts and
//! RPC bodies match the native HandoffService. Connections MUST be supplied by
//! the Host's live Bulletin WSS allowlist and session/permission fence. This
//! module accepts no endpoint and opens no connections.
//!
//! Every operation transfers at most one bounded entry. There is deliberately
//! no download-all loop, storage callback, or automatic acknowledgment. Persist
//! the ticket, routing identity, prepared ciphertext, root/progress and pending
//! acknowledgments in Host-private custody between effects. For compaction,
//! durable custody includes every embedded message and its complete claim plan.

#[cfg(test)]
use std::ops::Range;

use async_trait::async_trait;
use chacha20poly1305::aead::{AeadInPlace, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Nonce, Tag};
use parity_scale_codec::{Compact, Decode, Encode};
use schnorrkel::{ExpansionMode, Keypair, MiniSecretKey, signing_context};
use serde_json::{Value, json};
use zeroize::{Zeroize, ZeroizeOnDrop, Zeroizing};

pub const HOP_NOT_FOUND: i64 = 1_004;
pub const BITSWAP_NOT_FOUND: i64 = -32_810;
#[cfg(test)]
pub const BITSWAP_INVALID_CID: i64 = -32_602;
pub const HOP_CHUNK_BYTES: usize = 2_000_000;
pub const HOP_INLINE_MAX_BYTES: usize = HOP_CHUNK_BYTES - 64;
/// Native rich-attachment metadata uses an un-compacted u32 file size.
pub const HOP_MAX_FILE_BYTES: u64 = u32::MAX as u64;
const CRYPTO_OVERHEAD: usize = 12 + 16;
const MAX_ENCRYPTED_BYTES: usize = HOP_CHUNK_BYTES + CRYPTO_OVERHEAD;
// A root must itself fit a single entry. Each native Vec<u8> hash costs 33
// bytes; reserve the version/payload, u64 total, and maximum compact count.
const MAX_ROOT_CHUNKS: usize = (HOP_CHUNK_BYTES - 15) / 33;
const MAX_UPLOAD_CHUNKS: usize = (HOP_MAX_FILE_BYTES as usize / HOP_CHUNK_BYTES) + 1;
const SUBMIT_CONTEXT: &[u8] = b"hop-submit-v1:";
const CLAIM_CONTEXT: &[u8] = b"hop-claim-v1:";
const ACK_CONTEXT: &[u8] = b"hop-ack-v1:";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HopError {
    /// Preserve this code at the JSON-RPC boundary: only 1004 permits fallback.
    #[error("HOP RPC {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("HOP transport: {0}")]
    Transport(String),
    #[error("invalid HOP encoding: {0}")]
    Codec(String),
    #[error("HOP cryptographic operation failed")]
    Crypto,
    #[error("HOP entry integrity check failed")]
    Integrity,
    #[error("HOP entry is absent from pool and bitswap")]
    NotFound([u8; 32]),
    #[error("invalid HOP transfer progress")]
    InvalidProgress,
}

/// Implemented by the actor's fenced connection, never by a guest URL dialer.
#[async_trait]
pub trait HopRpc: Send + Sync {
    async fn call(&self, method: &str, params: Value) -> Result<Value, HopError>;
}

/// Native Host identities use Sr25519; preserve its HOP wire discriminant.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub enum MultiSigner {
    #[codec(index = 1)]
    Sr25519([u8; 32]),
}

/// Only the native Sr25519 signing scheme is accepted by this client.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub enum MultiSignature {
    #[codec(index = 1)]
    Sr25519([u8; 64]),
}

pub struct SenderProof {
    pub sender: MultiSigner,
    pub signature: MultiSignature,
    /// Unix milliseconds, included as little-endian u64 in the signed payload.
    pub submit_timestamp: u64,
}

/// The Host signs sender_proof_payload(hash, timestamp) through its native
/// session-fenced authority. No mnemonic or wallet key enters this module.
#[async_trait]
pub trait SenderProofProviding: Send + Sync {
    async fn proof(&self, data_hash: &[u8; 32]) -> Result<SenderProof, HopError>;
}

pub fn blake2b_256(data: &[u8]) -> [u8; 32] {
    let hash = blake2b_simd::Params::new().hash_length(32).hash(data);
    let mut output = [0; 32];
    output.copy_from_slice(hash.as_bytes());
    output
}

pub fn sender_proof_payload(hash: &[u8; 32], timestamp: u64) -> [u8; 32] {
    proof_payload(SUBMIT_CONTEXT, hash, &timestamp.to_le_bytes())
}

fn proof_payload(context: &[u8], hash: &[u8; 32], suffix: &[u8]) -> [u8; 32] {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .to_state()
        .update(context)
        .update(hash)
        .update(suffix)
        .finalize();
    let mut output = [0; 32];
    output.copy_from_slice(hash.as_bytes());
    output
}

/// A secret capability, deliberately neither Debug nor Copy nor serializable
/// through a guest-facing derive. as_bytes is for Host-private durable custody.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct FileTicket([u8; 32]);

impl FileTicket {
    pub fn generate() -> Result<Self, HopError> {
        let mut ticket = Self([0; 32]);
        getrandom::getrandom(&mut ticket.0).map_err(|_| HopError::Crypto)?;
        Ok(ticket)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, HopError> {
        if bytes.len() != 32 {
            return Err(codec("ticket must be 32 bytes"));
        }
        let mut ticket = Self([0; 32]);
        ticket.0.copy_from_slice(bytes);
        Ok(ticket)
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    fn derive_key(&self, context: &[u8]) -> Zeroizing<[u8; 32]> {
        let hash = blake2b_simd::Params::new()
            .hash_length(32)
            .key(&self.0)
            .hash(context);
        let mut key = Zeroizing::new([0; 32]);
        key.copy_from_slice(hash.as_bytes());
        key
    }

    fn signing_keypair(&self) -> Result<Keypair, HopError> {
        let seed = self.derive_key(b"signer");
        // schnorrkel's mini-secret and keypair both zeroize on drop.
        let mini = MiniSecretKey::from_bytes(&*seed).map_err(|_| HopError::Crypto)?;
        Ok(mini.expand_to_keypair(ExpansionMode::Ed25519))
    }

    #[cfg(test)]
    pub fn recipient(&self) -> Result<MultiSigner, HopError> {
        Ok(MultiSigner::Sr25519(
            self.signing_keypair()?.public.to_bytes(),
        ))
    }

    fn recipient_proof(
        &self,
        hash: &[u8; 32],
        context: &[u8],
    ) -> Result<([u8; 32], MultiSignature), HopError> {
        let keypair = self.signing_keypair()?;
        let payload = proof_payload(context, hash, &[]);
        let signature = keypair.sign(signing_context(b"substrate").bytes(&payload));
        Ok((
            keypair.public.to_bytes(),
            MultiSignature::Sr25519(signature.to_bytes()),
        ))
    }

    fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>, HopError> {
        if plaintext.len() > HOP_CHUNK_BYTES {
            return Err(codec("plaintext exceeds native entry bound"));
        }
        let mut nonce = [0; 12];
        getrandom::getrandom(&mut nonce).map_err(|_| HopError::Crypto)?;
        self.encrypt_with_nonce(plaintext, nonce)
    }

    fn encrypt_with_nonce(&self, plaintext: &[u8], nonce: [u8; 12]) -> Result<Vec<u8>, HopError> {
        let key = self.derive_key(b"encryption");
        let mut combined = Zeroizing::new(Vec::with_capacity(plaintext.len() + CRYPTO_OVERHEAD));
        combined.extend_from_slice(&nonce);
        combined.extend_from_slice(plaintext);
        let tag = ChaCha20Poly1305::new((&*key).into())
            .encrypt_in_place_detached(Nonce::from_slice(&nonce), &[], &mut combined[12..])
            .map_err(|_| HopError::Crypto)?;
        combined.extend_from_slice(&tag);
        Ok(std::mem::take(&mut *combined))
    }

    fn decrypt(&self, encrypted: &[u8]) -> Result<Zeroizing<Vec<u8>>, HopError> {
        if !(CRYPTO_OVERHEAD..=MAX_ENCRYPTED_BYTES).contains(&encrypted.len()) {
            return Err(codec("invalid encrypted entry length"));
        }
        let key = self.derive_key(b"encryption");
        let tag_start = encrypted.len() - 16;
        // In-place decryption may partially modify its buffer on failure; it
        // must already be zeroizing before authentication is attempted.
        let mut plaintext = Zeroizing::new(encrypted[12..tag_start].to_vec());
        ChaCha20Poly1305::new((&*key).into())
            .decrypt_in_place_detached(
                Nonce::from_slice(&encrypted[..12]),
                &[],
                &mut plaintext,
                Tag::from_slice(&encrypted[tag_start..]),
            )
            .map_err(|_| HopError::Crypto)?;
        Ok(plaintext)
    }
}

/// Exact ciphertext is persistable BEFORE submission. Retrying this object
/// reuses its nonce/hash rather than creating unreachable duplicate entries.
#[derive(Encode)]
pub struct PreparedUpload {
    hash: [u8; 32],
    recipient: [u8; 32],
    encrypted: Vec<u8>,
}

impl PreparedUpload {
    fn new(plaintext: &[u8], ticket: &FileTicket) -> Result<Self, HopError> {
        let recipient = ticket.signing_keypair()?.public.to_bytes();
        let encrypted = ticket.encrypt(plaintext)?;
        Ok(Self {
            hash: blake2b_256(&encrypted),
            recipient,
            encrypted,
        })
    }

    pub fn inline(data: &[u8], ticket: &FileTicket) -> Result<Self, HopError> {
        Self::new(&encode_inline(data)?, ticket)
    }

    /// Chunks are encrypted raw bytes, NOT nested root envelopes.
    pub fn chunk(data: &[u8], ticket: &FileTicket) -> Result<Self, HopError> {
        if data.is_empty() || data.len() > HOP_CHUNK_BYTES {
            return Err(codec("invalid native chunk length"));
        }
        Self::new(data, ticket)
    }

    // Peer-side compaction fixture construction; the Host only opens history.
    #[cfg(test)]
    pub fn compaction<M: AsRef<[u8]>>(
        messages: &[M],
        ticket: &FileTicket,
    ) -> Result<Self, HopError> {
        Self::new(&encode_compaction_entry(messages)?, ticket)
    }

    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    /// Strict bounded inverse of Encode for Host-private restart records.
    pub fn restore(bytes: &[u8]) -> Result<Self, HopError> {
        let mut input = bytes;
        let hash = fixed::<32>(&mut input)?;
        let recipient = fixed::<32>(&mut input)?;
        let encrypted = byte_vector(&mut input, MAX_ENCRYPTED_BYTES)?;
        finish(input)?;
        if encrypted.len() < CRYPTO_OVERHEAD || blake2b_256(encrypted) != hash {
            return Err(HopError::Integrity);
        }
        Ok(Self {
            hash,
            recipient,
            encrypted: encrypted.to_vec(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolStatus {
    pub entry_count: u64,
    pub total_bytes: u64,
    pub max_bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubmittedData {
    pub hash: [u8; 32],
    pub pool_status: PoolStatus,
}

/// Only successful authenticated POOL claims create these. Bitswap claims
/// return None, so promoted entries cannot accidentally acquire an ack token.
/// Persist with the routing identity and ticket before acknowledging. Replaying
/// an ack after a crash is safe: native 1004 is success.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct PendingAck {
    hash: [u8; 32],
    recipient: [u8; 32],
}

impl PendingAck {
    #[cfg(test)]
    pub fn hash(&self) -> [u8; 32] {
        self.hash
    }

    pub fn restore(bytes: &[u8]) -> Result<Self, HopError> {
        let mut input = bytes;
        let value = Self {
            hash: fixed(&mut input)?,
            recipient: fixed(&mut input)?,
        };
        finish(input)?;
        Ok(value)
    }
}

/// Native chunk metadata has Vec<Vec<u8>> hashes, not Vec<[u8; 32]>.
/// The private in-memory representation avoids an allocation for each hash.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct ChunkedFile {
    total_size: u64,
    chunks: Vec<[u8; 32]>,
}

impl ChunkedFile {
    #[cfg(test)]
    pub fn total_size(&self) -> u64 {
        self.total_size
    }

    #[cfg(test)]
    pub fn chunks(&self) -> &[[u8; 32]] {
        &self.chunks
    }

    fn validate(&self) -> Result<(), HopError> {
        validate_chunk_layout(self.total_size, self.chunks.len())
    }
}

/// Persist this descriptor separately from clear inline file bytes. Encode is
/// the Host-private resume codec, NOT the native pool envelope codec.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub enum RootDescriptor {
    #[codec(index = 0)]
    Inline { entry_hash: [u8; 32], byte_len: u32 },
    #[codec(index = 1)]
    Chunked {
        entry_hash: [u8; 32],
        metadata: ChunkedFile,
    },
}

impl RootDescriptor {
    pub fn entry_hash(&self) -> [u8; 32] {
        match self {
            Self::Inline { entry_hash, .. } | Self::Chunked { entry_hash, .. } => *entry_hash,
        }
    }

    pub fn total_size(&self) -> u64 {
        match self {
            Self::Inline { byte_len, .. } => u64::from(*byte_len),
            Self::Chunked { metadata, .. } => metadata.total_size,
        }
    }

    pub fn restore(bytes: &[u8]) -> Result<Self, HopError> {
        let mut input = bytes;
        let kind = fixed::<1>(&mut input)?[0];
        let entry_hash = fixed(&mut input)?;
        let result = match kind {
            0 => {
                let byte_len = u32::from_le_bytes(fixed(&mut input)?);
                if byte_len as usize > HOP_INLINE_MAX_BYTES {
                    return Err(codec("inline descriptor exceeds native bound"));
                }
                Self::Inline {
                    entry_hash,
                    byte_len,
                }
            }
            1 => {
                let total_size = u64::from_le_bytes(fixed(&mut input)?);
                let count = compact(&mut input)? as usize;
                validate_chunk_layout(total_size, count)?;
                if input.len() != count * 32 {
                    return Err(codec("invalid persisted chunk hash vector"));
                }
                let mut chunks = Vec::with_capacity(count);
                for _ in 0..count {
                    chunks.push(fixed(&mut input)?);
                }
                Self::Chunked {
                    entry_hash,
                    metadata: ChunkedFile { total_size, chunks },
                }
            }
            _ => return Err(codec("unknown root descriptor kind")),
        };
        finish(input)?;
        Ok(result)
    }
}

pub struct ClaimedRoot {
    pub descriptor: RootDescriptor,
    /// Some only for inline roots. The caller must durably save these bytes.
    pub inline: Option<Zeroizing<Vec<u8>>>,
    pub pending_ack: Option<PendingAck>,
}

/// Sequential resumable chunk download. It never represents inline files.
/// Native peers may choose smaller chunks: offsets are observed byte counts,
/// not index * HOP_CHUNK_BYTES. The final chunk must exactly match total_size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Encode)]
pub struct DownloadProgress {
    pub next_chunk: u32,
    pub downloaded_bytes: u64,
}

impl DownloadProgress {
    pub fn restore(bytes: &[u8], root: &RootDescriptor) -> Result<Self, HopError> {
        let mut input = bytes;
        let value = Self {
            next_chunk: u32::from_le_bytes(fixed(&mut input)?),
            downloaded_bytes: u64::from_le_bytes(fixed(&mut input)?),
        };
        finish(input)?;
        value.validate(root)?;
        Ok(value)
    }

    pub fn validate(&self, root: &RootDescriptor) -> Result<(), HopError> {
        let RootDescriptor::Chunked { metadata, .. } = root else {
            return Err(HopError::InvalidProgress);
        };
        metadata.validate()?;
        let index = self.next_chunk as usize;
        if index > metadata.chunks.len() || self.downloaded_bytes > metadata.total_size {
            return Err(HopError::InvalidProgress);
        }
        let remaining_chunks = (metadata.chunks.len() - index) as u64;
        let remaining_bytes = metadata.total_size - self.downloaded_bytes;
        let completed = u64::from(self.next_chunk);
        if self.downloaded_bytes < completed
            || self.downloaded_bytes > completed * HOP_CHUNK_BYTES as u64
            || remaining_bytes < remaining_chunks
            || remaining_bytes > remaining_chunks * HOP_CHUNK_BYTES as u64
        {
            return Err(HopError::InvalidProgress);
        }
        Ok(())
    }

    pub fn is_complete(&self, root: &RootDescriptor) -> Result<bool, HopError> {
        self.validate(root)?;
        Ok(self.downloaded_bytes == root.total_size())
    }
}

pub struct ClaimedChunk {
    pub index: u32,
    pub data: Zeroizing<Vec<u8>>,
    /// Persist together with data before separately acknowledging pending_ack.
    pub next_progress: DownloadProgress,
    pub pending_ack: Option<PendingAck>,
}

/// Upload progress only for files over the native inline threshold. Each
/// successful submit records one full 2MB chunk, except the final remainder.
#[derive(Debug, Clone, PartialEq, Eq, Encode)]
pub struct UploadProgress {
    total_size: u32,
    uploaded_hashes: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRequest {
    pub index: u32,
    pub offset: u64,
    pub byte_len: usize,
}

impl UploadProgress {
    pub fn new(total_size: u32) -> Result<Self, HopError> {
        if total_size as usize <= HOP_INLINE_MAX_BYTES {
            return Err(HopError::InvalidProgress);
        }
        Ok(Self {
            total_size,
            uploaded_hashes: Vec::new(),
        })
    }

    pub fn total_size(&self) -> u32 {
        self.total_size
    }

    pub fn uploaded_size(&self) -> u64 {
        (self.uploaded_hashes.len() as u64 * HOP_CHUNK_BYTES as u64).min(u64::from(self.total_size))
    }

    pub fn restore(bytes: &[u8]) -> Result<Self, HopError> {
        let mut input = bytes;
        let total_size = u32::from_le_bytes(fixed(&mut input)?);
        let mut value = Self::new(total_size)?;
        let count = compact(&mut input)? as usize;
        let expected = u64::from(total_size).div_ceil(HOP_CHUNK_BYTES as u64) as usize;
        if count > expected || count > MAX_UPLOAD_CHUNKS || input.len() != count * 32 {
            return Err(HopError::InvalidProgress);
        }
        value.uploaded_hashes.reserve(count);
        for _ in 0..count {
            value.uploaded_hashes.push(fixed(&mut input)?);
        }
        finish(input)?;
        Ok(value)
    }

    pub fn next_chunk(&self) -> Option<ChunkRequest> {
        let offset = self.uploaded_hashes.len() as u64 * HOP_CHUNK_BYTES as u64;
        let remaining = u64::from(self.total_size).checked_sub(offset)?;
        if remaining == 0 {
            return None;
        }
        Some(ChunkRequest {
            index: self.uploaded_hashes.len() as u32,
            offset,
            byte_len: remaining.min(HOP_CHUNK_BYTES as u64) as usize,
        })
    }

    /// Call only after submit succeeds, and persist before requesting the next
    /// chunk. Keep the PreparedUpload until this record is durable.
    pub fn record_chunk(&mut self, hash: [u8; 32], byte_len: usize) -> Result<(), HopError> {
        let expected = self.next_chunk().ok_or(HopError::InvalidProgress)?;
        if byte_len != expected.byte_len {
            return Err(HopError::InvalidProgress);
        }
        self.uploaded_hashes.push(hash);
        Ok(())
    }

    pub fn prepare_root(&self, ticket: &FileTicket) -> Result<PreparedUpload, HopError> {
        if self.next_chunk().is_some() {
            return Err(HopError::InvalidProgress);
        }
        PreparedUpload::new(
            &encode_chunked_envelope(u64::from(self.total_size), &self.uploaded_hashes)?,
            ticket,
        )
    }
}

pub struct ClaimedCompaction {
    pub batch: CompactionBatch,
    pub pending_ack: Option<PendingAck>,
}

/// One zeroizing allocation owns the clear entry. Messages borrow its bytes;
/// no Vec per message, even for the maximum valid number of empty messages.
/// Intentionally not Debug: compacted messages are Host-private cleartext.
pub struct CompactionBatch {
    entry: Zeroizing<Vec<u8>>,
    messages_start: usize,
    message_count: u32,
}

impl CompactionBatch {
    #[cfg(test)]
    pub fn message_count(&self) -> usize {
        self.message_count as usize
    }

    pub fn messages(&self) -> impl Iterator<Item = &[u8]> {
        let mut input = &self.entry[self.messages_start..];
        (0..self.message_count).map(move |_| {
            // Validated in full before CompactionBatch can be constructed;
            // callers have no mutable access to its backing allocation.
            byte_vector(&mut input, HOP_INLINE_MAX_BYTES)
                .expect("validated immutable compaction batch")
        })
    }
}

/// Return ordered ranges into the caller's messages, never duplicate payloads.
/// The exact SCALE outer count prefix is included at every size boundary.
#[cfg(test)]
pub fn pack_compaction_batches<M: AsRef<[u8]>>(
    messages: &[M],
) -> Result<Vec<Range<usize>>, HopError> {
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut payload_size = 0;
    for (index, message) in messages.iter().enumerate() {
        let size = encoded_message_size(message.as_ref())?;
        if 1 + size > HOP_INLINE_MAX_BYTES {
            return Err(codec("message cannot fit an inline compaction batch"));
        }
        let count = index - start + 1;
        if compact_size(count as u32) + payload_size + size > HOP_INLINE_MAX_BYTES {
            ranges.push(start..index);
            start = index;
            payload_size = 0;
        }
        payload_size += size;
    }
    if start < messages.len() {
        ranges.push(start..messages.len());
    }
    Ok(ranges)
}

#[cfg(test)]
pub fn encode_compaction_entry<M: AsRef<[u8]>>(
    messages: &[M],
) -> Result<Zeroizing<Vec<u8>>, HopError> {
    if messages.len() > HOP_INLINE_MAX_BYTES {
        return Err(codec("compaction count exceeds inline bound"));
    }
    let count = messages.len() as u32;
    let mut size = compact_size(count);
    for message in messages {
        size += encoded_message_size(message.as_ref())?;
        if size > HOP_INLINE_MAX_BYTES {
            return Err(codec("compaction batch exceeds inline bound"));
        }
    }
    let mut encoded = Zeroizing::new(Vec::with_capacity(2 + compact_size(size as u32) + size));
    encoded.extend_from_slice(&[0, 0]);
    Compact(size as u32).encode_to(&mut *encoded);
    Compact(count).encode_to(&mut *encoded);
    for message in messages {
        let bytes = message.as_ref();
        Compact(bytes.len() as u32).encode_to(&mut *encoded);
        encoded.extend_from_slice(bytes);
    }
    Ok(encoded)
}

/// Reject chunked entries, unknown versions, impossible/noncanonical lengths
/// and trailing bytes before exposing any inner message or allocating a vector.
pub fn decode_compaction_entry(entry: Zeroizing<Vec<u8>>) -> Result<CompactionBatch, HopError> {
    let mut input = entry.as_slice();
    if fixed::<2>(&mut input)? != [0, 0] {
        return Err(codec("compaction requires a version-0 inline envelope"));
    }
    let mut batch = byte_vector(&mut input, HOP_INLINE_MAX_BYTES)?;
    finish(input)?;
    let message_count = compact(&mut batch)?;
    if message_count as usize > batch.len() {
        return Err(codec("compaction count exceeds remaining bytes"));
    }
    let messages_start = entry.len() - batch.len();
    for _ in 0..message_count {
        byte_vector(&mut batch, HOP_INLINE_MAX_BYTES)?;
    }
    finish(batch)?;
    Ok(CompactionBatch {
        entry,
        messages_start,
        message_count,
    })
}

pub struct HopClient<'a> {
    rpc: &'a dyn HopRpc,
}

impl<'a> HopClient<'a> {
    pub fn new(rpc: &'a dyn HopRpc) -> Self {
        Self { rpc }
    }

    pub async fn submit(
        &self,
        prepared: &PreparedUpload,
        sender: &dyn SenderProofProviding,
    ) -> Result<SubmittedData, HopError> {
        let proof = sender.proof(&prepared.hash).await?;
        let result = self.rpc.call("hop_submit", json!({
            "data": prefixed_hex(&prepared.encrypted),
            "recipients": [prefixed_hex(&MultiSigner::Sr25519(prepared.recipient).encode())],
            "signature": prefixed_hex(&proof.signature.encode()),
            "signer": prefixed_hex(&proof.sender.encode()),
            "submit_timestamp": proof.submit_timestamp,
        })).await?;
        let status = result
            .get("poolStatus")
            .ok_or_else(|| codec("missing poolStatus"))?;
        let field = |name: &str| {
            status
                .get(name)
                .and_then(Value::as_u64)
                .ok_or_else(|| codec("invalid poolStatus field"))
        };
        Ok(SubmittedData {
            hash: prepared.hash,
            pool_status: PoolStatus {
                entry_count: field("entryCount")?,
                total_bytes: field("totalBytes")?,
                max_bytes: field("maxBytes")?,
            },
        })
    }

    pub async fn claim_root(
        &self,
        entry_hash: [u8; 32],
        ticket: &FileTicket,
        expected_size: Option<u32>,
    ) -> Result<ClaimedRoot, HopError> {
        let (mut plaintext, pending_ack) = self.claim(&entry_hash, ticket).await?;
        let (descriptor, inline_len) = match decode_root(&plaintext)? {
            DecodedRoot::Inline(data) => (
                RootDescriptor::Inline {
                    entry_hash,
                    byte_len: data.len() as u32,
                },
                Some(data.len()),
            ),
            DecodedRoot::Chunked(metadata) => (
                RootDescriptor::Chunked {
                    entry_hash,
                    metadata,
                },
                None,
            ),
        };
        if expected_size.is_some_and(|size| u64::from(size) != descriptor.total_size()) {
            return Err(codec("attachment size does not match root"));
        }
        let inline = inline_len.map(|len| {
            // Strip the envelope without allocating a second clear file.
            let start = plaintext.len() - len;
            plaintext.copy_within(start.., 0);
            plaintext[len..].zeroize();
            plaintext.truncate(len);
            plaintext
        });
        Ok(ClaimedRoot {
            descriptor,
            inline,
            pending_ack,
        })
    }

    /// A completed progress record yields None without making a network call.
    /// Otherwise this claims exactly one chunk and returns a proposed next
    /// record; no in-memory or durable progress is advanced on the caller's behalf.
    pub async fn claim_chunk(
        &self,
        root: &RootDescriptor,
        progress: DownloadProgress,
        ticket: &FileTicket,
    ) -> Result<Option<ClaimedChunk>, HopError> {
        progress.validate(root)?;
        let RootDescriptor::Chunked { metadata, .. } = root else {
            return Err(HopError::InvalidProgress);
        };
        let Some(hash) = metadata.chunks.get(progress.next_chunk as usize) else {
            return Ok(None);
        };
        let (data, pending_ack) = self.claim(hash, ticket).await?;
        let next_progress = DownloadProgress {
            next_chunk: progress.next_chunk + 1,
            downloaded_bytes: progress.downloaded_bytes + data.len() as u64,
        };
        if data.is_empty() {
            return Err(codec("empty native file chunk"));
        }
        next_progress.validate(root)?;
        Ok(Some(ClaimedChunk {
            index: progress.next_chunk,
            data,
            next_progress,
            pending_ack,
        }))
    }

    pub async fn claim_compaction(
        &self,
        entry_hash: [u8; 32],
        ticket: &FileTicket,
    ) -> Result<ClaimedCompaction, HopError> {
        let (plaintext, pending_ack) = self.claim(&entry_hash, ticket).await?;
        let batch = decode_compaction_entry(plaintext)?;
        Ok(ClaimedCompaction { batch, pending_ack })
    }

    /// Only the actor calls this, after durable custody / complete claim plans.
    /// It is never invoked by claim, decode, progress, drop, or bitswap fallback.
    pub async fn acknowledge(
        &self,
        pending: &PendingAck,
        ticket: &FileTicket,
    ) -> Result<(), HopError> {
        let (recipient, signature) = ticket.recipient_proof(&pending.hash, ACK_CONTEXT)?;
        if recipient != pending.recipient {
            return Err(HopError::Crypto);
        }
        match self
            .rpc
            .call(
                "hop_ack",
                json!({
                    "raw_hash": prefixed_hex(&pending.hash),
                    "signature": prefixed_hex(&signature.encode()),
                }),
            )
            .await
        {
            Ok(_) => Ok(()),
            Err(HopError::Rpc {
                code: HOP_NOT_FOUND,
                ..
            }) => Ok(()),
            Err(error) => Err(error),
        }
    }

    async fn claim(
        &self,
        hash: &[u8; 32],
        ticket: &FileTicket,
    ) -> Result<(Zeroizing<Vec<u8>>, Option<PendingAck>), HopError> {
        let (recipient, signature) = ticket.recipient_proof(hash, CLAIM_CONTEXT)?;
        let (result, pool) = match self
            .rpc
            .call(
                "hop_claim",
                json!({
                    "raw_hash": prefixed_hex(hash),
                    "signature": prefixed_hex(&signature.encode()),
                }),
            )
            .await
        {
            Ok(result) => (result, true),
            Err(HopError::Rpc {
                code: HOP_NOT_FOUND,
                ..
            }) => {
                let result = match self
                    .rpc
                    .call("bitswap_v1_get", json!([raw_cid(hash)]))
                    .await
                {
                    Ok(result) => result,
                    Err(HopError::Rpc {
                        code: BITSWAP_NOT_FOUND,
                        ..
                    }) => return Err(HopError::NotFound(*hash)),
                    // Invalid CID, permissions, timeouts and every other error
                    // propagate intact. No secondary source or blind retry.
                    Err(error) => return Err(error),
                };
                (result, false)
            }
            Err(error) => return Err(error),
        };
        let encrypted = decode_rpc_bytes(&result)?;
        if blake2b_256(&encrypted) != *hash {
            return Err(HopError::Integrity);
        }
        let plaintext = ticket.decrypt(&encrypted)?;
        let pending = pool.then_some(PendingAck {
            hash: *hash,
            recipient,
        });
        Ok((plaintext, pending))
    }
}

fn codec(message: &str) -> HopError {
    HopError::Codec(message.into())
}

fn prefixed_hex(bytes: &[u8]) -> String {
    let mut encoded = String::with_capacity(2 + bytes.len() * 2);
    encoded.push_str("0x");
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in bytes {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 15) as usize] as char);
    }
    encoded
}

fn decode_rpc_bytes(value: &Value) -> Result<Vec<u8>, HopError> {
    let value = value
        .as_str()
        .ok_or_else(|| codec("entry result must be hex text"))?;
    let hex = value.strip_prefix("0x").unwrap_or(value);
    if hex.len() > MAX_ENCRYPTED_BYTES * 2 || hex.len() < CRYPTO_OVERHEAD * 2 {
        return Err(codec("entry result exceeds native size bounds"));
    }
    // hex::decode handles malformed Unicode without byte-indexing panics.
    hex::decode(hex).map_err(|_| codec("invalid hex entry"))
}

fn fixed<const N: usize>(input: &mut &[u8]) -> Result<[u8; N], HopError> {
    if input.len() < N {
        return Err(codec("truncated fixed field"));
    }
    let mut value = [0; N];
    value.copy_from_slice(&input[..N]);
    *input = &input[N..];
    Ok(value)
}

fn compact(input: &mut &[u8]) -> Result<u32, HopError> {
    let before = input.len();
    let value = Compact::<u32>::decode(input)
        .map_err(|_| codec("invalid SCALE compact length"))?
        .0;
    if before - input.len() != compact_size(value) {
        return Err(codec("noncanonical SCALE compact length"));
    }
    Ok(value)
}

fn compact_size(value: u32) -> usize {
    match value {
        0..=63 => 1,
        64..=16_383 => 2,
        16_384..=1_073_741_823 => 4,
        _ => 5,
    }
}

fn byte_vector<'a>(input: &mut &'a [u8], maximum: usize) -> Result<&'a [u8], HopError> {
    let size = compact(input)? as usize;
    if size > maximum || size > input.len() {
        return Err(codec("SCALE vector exceeds bounded input"));
    }
    let (bytes, remaining) = input.split_at(size);
    *input = remaining;
    Ok(bytes)
}

fn finish(input: &[u8]) -> Result<(), HopError> {
    if input.is_empty() {
        Ok(())
    } else {
        Err(codec("trailing bytes"))
    }
}

#[cfg(test)]
fn encoded_message_size(message: &[u8]) -> Result<usize, HopError> {
    if message.len() > HOP_INLINE_MAX_BYTES {
        return Err(codec("message exceeds inline bound"));
    }
    Ok(compact_size(message.len() as u32) + message.len())
}

fn encode_inline(data: &[u8]) -> Result<Zeroizing<Vec<u8>>, HopError> {
    if data.len() > HOP_INLINE_MAX_BYTES {
        return Err(codec("inline file exceeds native bound"));
    }
    let mut encoded = Zeroizing::new(Vec::with_capacity(
        2 + compact_size(data.len() as u32) + data.len(),
    ));
    encoded.extend_from_slice(&[0, 0]);
    Compact(data.len() as u32).encode_to(&mut *encoded);
    encoded.extend_from_slice(data);
    Ok(encoded)
}

fn encode_chunked_envelope(
    total_size: u64,
    chunks: &[[u8; 32]],
) -> Result<Zeroizing<Vec<u8>>, HopError> {
    validate_chunk_layout(total_size, chunks.len())?;
    let mut encoded = Zeroizing::new(Vec::with_capacity(15 + chunks.len() * 33));
    encoded.extend_from_slice(&[0, 1]);
    total_size.encode_to(&mut *encoded);
    Compact(chunks.len() as u32).encode_to(&mut *encoded);
    for hash in chunks {
        Compact(32u32).encode_to(&mut *encoded);
        encoded.extend_from_slice(hash);
    }
    Ok(encoded)
}

fn validate_chunk_layout(total_size: u64, count: usize) -> Result<(), HopError> {
    if total_size == 0
        || total_size > HOP_MAX_FILE_BYTES
        || count == 0
        || count > MAX_ROOT_CHUNKS
        || total_size < count as u64
        || total_size > count as u64 * HOP_CHUNK_BYTES as u64
    {
        return Err(codec("invalid native chunked layout"));
    }
    Ok(())
}

enum DecodedRoot<'a> {
    Inline(&'a [u8]),
    Chunked(ChunkedFile),
}

fn decode_root(entry: &[u8]) -> Result<DecodedRoot<'_>, HopError> {
    if entry.len() > HOP_CHUNK_BYTES {
        return Err(codec("root exceeds native entry bound"));
    }
    let mut input = entry;
    let header = fixed::<2>(&mut input)?;
    if header[0] != 0 {
        return Err(codec("unsupported root envelope version"));
    }
    let result = match header[1] {
        0 => DecodedRoot::Inline(byte_vector(&mut input, HOP_INLINE_MAX_BYTES)?),
        1 => {
            let total_size = u64::from_le_bytes(fixed(&mut input)?);
            let count = compact(&mut input)? as usize;
            validate_chunk_layout(total_size, count)?;
            // Every canonical 32-byte Vec has a one-byte 0x80 prefix.
            // Preflight the complete encoded size before allocating any hashes.
            if input.len() != count * 33 {
                return Err(codec("invalid native chunk hash vector size"));
            }
            let mut chunks = Vec::with_capacity(count);
            for _ in 0..count {
                let hash = byte_vector(&mut input, 32)?;
                chunks.push(
                    hash.try_into()
                        .map_err(|_| codec("chunk hash must be 32 bytes"))?,
                );
            }
            DecodedRoot::Chunked(ChunkedFile { total_size, chunks })
        }
        _ => return Err(codec("unsupported root payload kind")),
    };
    finish(input)?;
    Ok(result)
}

/// CIDv1 / raw / BLAKE2b-256, lowercase unpadded base32. The six-byte prefix
/// is varint(1), varint(0x55), varint(0xb220), varint(32).
fn raw_cid(hash: &[u8; 32]) -> String {
    const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
    let mut bytes = [0; 38];
    bytes[..6].copy_from_slice(&[0x01, 0x55, 0xa0, 0xe4, 0x02, 0x20]);
    bytes[6..].copy_from_slice(hash);
    let mut cid = String::with_capacity(62);
    cid.push('b');
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in bytes {
        buffer = (buffer << 8) | u32::from(byte);
        bits += 8;
        while bits >= 5 {
            bits -= 5;
            cid.push(ALPHABET[((buffer >> bits) & 31) as usize] as char);
        }
    }
    if bits > 0 {
        cid.push(ALPHABET[((buffer << (5 - bits)) & 31) as usize] as char);
    }
    cid
}

#[cfg(test)]
mod tests;
