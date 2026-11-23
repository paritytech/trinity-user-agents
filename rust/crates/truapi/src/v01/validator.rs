//! Experimental host-custodied JAM validator protocol (trait 112).

use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

/// Largest complete recovery blob, in bytes.
pub const VALIDATOR_MAX_RECOVERY_BYTES: u32 = 64 * 1024 * 1024;
/// Largest recovery transfer chunk, in bytes.
pub const VALIDATOR_MAX_CHUNK_BYTES: u32 = 256 * 1024;
/// Largest encoded protocol-parameter set.
pub const VALIDATOR_MAX_PARAMETERS_BYTES: usize = 1024;
/// Largest waiting validator ring.
pub const VALIDATOR_MAX_RING: usize = 1023;
/// Maximum number of retained signing guards in one transfer page.
pub const VALIDATOR_MAX_GUARD_PAGE: u32 = 64;

/// Fail-closed custody errors. The runtime sanitizes private callback failures.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostValidatorError {
	/// The host has no protected validator backend.
	Unsupported,
	/// This execution/account/artifact has no signing grant.
	NotGranted,
	/// The execution-local handle is no longer owned.
	Closed,
	/// Another owner holds this identity.
	Busy,
	/// A transfer, storage, journal, or admission bound was exceeded.
	Limit,
	/// Malformed fields, noncanonical decimal u64, or invalid transfer order.
	Invalid,
	/// Expected revision or head does not match durable custody state.
	Stale,
	/// The duty conflicts with a retained signing intent.
	Conflict,
	/// Complete recovery material or replay acknowledgement is missing.
	RecoveryRequired,
	/// Missing/corrupt/ambiguous durable state; no signing is allowed.
	Poisoned,
	/// A trusted host persistence or keyring operation failed.
	Unavailable,
	/// Private platform bridge failure; runtime converts this to Poisoned and revokes.
	Unknown {
		/// Host-private bridge diagnostic, never forwarded to the guest.
		reason: String,
	},
}

/// Content-addressed complete recovery material; hashes are Blake2b-256.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorRecoveryRef {
	/// Complete blob hash.
	pub hash: [u8; 32],
	/// Complete byte length, at most 64 MiB.
	pub length: u32,
}

/// One supported GRANDPA signing domain. No arbitrary-byte signing exists.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorGrandpaDuty {
	/// 0 prevote, 1 precommit, 2 proposal; all other values refuse.
	pub stage: u8,
	/// Authority set identifier.
	pub set: u32,
	/// Canonical unsigned decimal u64, never a JSON number.
	pub round: String,
	/// Target header hash.
	pub target: [u8; 32],
	/// Target posterior state root, independently computed by execution.
	pub state_root: [u8; 32],
	/// Target slot.
	pub slot: u32,
}

/// Retained durable signing intent needed by guest recovery.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorIntent {
	/// Canonical unsigned decimal u64 journal revision.
	pub revision: String,
	/// Exact domain that was durably accepted.
	pub duty: ValidatorGrandpaDuty,
	/// Complete recovery material bound before signing.
	pub recovery: ValidatorRecoveryRef,
}

/// Runtime-owned authority, passed only to trusted platform callbacks.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorAuthority {
	/// Verified product namespace from the host connection.
	pub product_id: String,
	/// Current root account from runtime authority, not the guest.
	pub account: [u8; 32],
	/// Canonical unsigned decimal runtime instance id.
	pub execution_id: String,
}

/// Acquire custody, read recovery material, or acknowledge completed guest replay.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostValidatorOpenRequest {
	/// Acquire an existing host-enrolled identity, never enroll/import a key.
	Acquire {
		/// Immutable enrolled chain genesis.
		genesis: [u8; 32],
	},
	/// Read a bounded range of a retained recovery blob.
	Read {
		/// Opaque execution-local handle.
		handle: String,
		/// Complete retained blob reference.
		recovery: ValidatorRecoveryRef,
		/// Byte offset.
		offset: u32,
		/// Maximum bytes, at most 256 KiB.
		max_bytes: u32,
	},
	/// Guest has replayed/revalidated the acquired head; not proof of consensus.
	Ready {
		/// Opaque execution-local handle.
		handle: String,
		/// Replayed canonical decimal u64 revision.
		revision: String,
		/// Replayed journal head digest.
		head: [u8; 32],
	},
	/// Read one exact-head page; no full-history array is acquired.
	Retained {
		/// Opaque execution-local handle.
		handle: String,
		/// Expected canonical decimal revision.
		revision: String,
		/// Expected custody digest.
		head: [u8; 32],
		/// Global ordinal, intents before seals.
		offset: u32,
	},
	/// Look up every historical conflict for one GRANDPA duty key.
	LookupIntent {
		/// Opaque execution-local handle.
		handle: String,
		/// Expected canonical decimal revision.
		revision: String,
		/// Expected custody digest.
		head: [u8; 32],
		/// GRANDPA stage.
		stage: u8,
		/// Authority set.
		set: u32,
		/// Canonical decimal round.
		round: String,
	},
	/// Look up a consumed authoring slot, including archived history.
	LookupSeal {
		/// Opaque execution-local handle.
		handle: String,
		/// Expected canonical decimal revision.
		revision: String,
		/// Expected custody digest.
		head: [u8; 32],
		/// Consensus slot.
		slot: u32,
	},
	/// Read an immutable lossless archive object, not a live recovery witness.
	ArchiveRead {
		/// Opaque execution-local handle.
		handle: String,
		/// Content-addressed archive object.
		recovery: ValidatorRecoveryRef,
		/// Byte offset.
		offset: u32,
		/// At most 256 KiB.
		max_bytes: u32,
	},
	/// Authenticate an immutable archive object's hash and complete length.
	ArchiveInfo {
		/// Opaque execution-local handle.
		handle: String,
		/// Exact content hash, never a filesystem path.
		hash: [u8; 32],
	},
	/// Look up a full-duty conflict key, including archived history.
	LookupFull {
		/// Owned execution-local handle.
		handle: String,
		/// Expected canonical decimal revision.
		revision: String,
		/// Expected custody digest.
		head: [u8; 32],
		/// Exact typed domain whose guard is requested.
		duty: ValidatorFullDuty,
	},
}

/// Custody acquisition or bounded recovery result.
// Preserve the prototype's unboxed wire variants.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostValidatorOpenResponse {
	/// Locked, intact custody state, not yet replay-ready.
	Acquired {
		/// Opaque execution-local handle.
		handle: String,
		/// Enrolled Ed25519 public identity.
		ed25519: [u8; 32],
		/// Enrolled Bandersnatch public identity.
		bandersnatch: [u8; 32],
		/// Compressed P-256 public identity.
		p256: [u8; 33],
		/// Immutable enrolled genesis.
		genesis: [u8; 32],
		/// Immutable enrolled parameter digest.
		parameters: [u8; 32],
		/// Canonical decimal u64 key generation.
		generation: String,
		/// Canonical decimal u64 durable revision.
		revision: String,
		/// Durable journal head digest.
		head: [u8; 32],
		/// Latest complete recovery material, absent only before first commit.
		recovery: Option<ValidatorRecoveryRef>,
		/// Total retained GRANDPA intents; entries are paged or looked up.
		intent_count: u32,
		/// Total retained consumed slots.
		seal_count: u32,
		/// Retained full-duty guards, accessed only by typed lookup.
		full_duty_count: u32,
		/// Explicit trusted role-4 consent, never implied by authoring.
		full_duties: bool,
		/// Greatest consumed authoring slot, absent when seal_count is zero.
		latest_seal_slot: Option<u32>,
		/// Authenticated immutable history/index root at this custody head.
		history_root: [u8; 32],
		/// Immutable role-3 enrollment, distinct from vote-only custody.
		authoring: bool,
		/// Host Unix milliseconds sampled at acquisition; guest owns slot interpretation.
		unix_ms: String,
		/// Explicit software protection class, never a hardware claim.
		protection: ValidatorProtection,
		/// Supported GRANDPA stage numbers.
		stages: Vec<u8>,
		/// Maximum complete recovery blob size.
		max_recovery_bytes: u32,
		/// Maximum transfer chunk size.
		max_chunk_bytes: u32,
		/// Canonical decimal u64 aggregate storage quota.
		max_storage_bytes: String,
		/// Canonical decimal byte quota for signing history and indexes.
		max_history_bytes: String,
		/// Canonical decimal byte quota for lossless archive objects.
		max_archive_bytes: String,
	},
	/// Bounded bytes starting at the requested offset.
	Read {
		/// Recovery chunk, never key material.
		bytes: Vec<u8>,
	},
	/// The exact head was acknowledged for replay.
	Ready,
	/// Exact-head bounded retained-history page.
	Retained {
		/// Canonical decimal custody revision.
		revision: String,
		/// Custody digest.
		head: [u8; 32],
		/// Echoed global ordinal.
		offset: u32,
		/// At most 64 total intents and seals.
		intents: Vec<ValidatorIntent>,
		/// Remainder of the same page after its intents.
		seals: Vec<ValidatorSealIntent>,
		/// Next ordinal, absent only at the exact end.
		next: Option<u32>,
	},
	/// Authenticated historical GRANDPA lookup; absence is not a storage error.
	Intent {
		/// Canonical decimal custody revision.
		revision: String,
		/// Custody digest.
		head: [u8; 32],
		/// Exact retained duty if present.
		intent: Option<ValidatorIntent>,
	},
	/// Authenticated historical consumed-slot lookup.
	Seal {
		/// Canonical decimal custody revision.
		revision: String,
		/// Custody digest.
		head: [u8; 32],
		/// Exact retained seal if present.
		seal: Option<ValidatorSealIntent>,
	},
	/// Verified immutable archive metadata.
	ArchiveInfo {
		/// Content hash and authenticated byte length.
		recovery: ValidatorRecoveryRef,
	},
	/// Exact-head retained full-duty lookup.
	Full {
		/// Canonical decimal custody revision.
		revision: String,
		/// Custody digest.
		head: [u8; 32],
		/// Retained guard for the requested conflict key, if present.
		intent: Option<ValidatorFullIntent>,
	},
}

/// Honest protection classes offered by this experiment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ValidatorProtection {
	/// OS keyring wraps a software seed, which exists in trusted host memory.
	OsKeyringWrappedSoftware,
}

/// Sequential bounded upload or a durable compare-and-append checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostValidatorCommitRequest {
	/// Start the sole in-progress upload; does not create a signing intent.
	Begin {
		/// Opaque execution-local handle.
		handle: String,
		/// Declared complete material hash and size.
		recovery: ValidatorRecoveryRef,
	},
	/// Append the next sequential chunk.
	Chunk {
		/// Opaque execution-local handle.
		handle: String,
		/// Exact next byte offset.
		offset: u32,
		/// At most 256 KiB.
		bytes: Vec<u8>,
	},
	/// Verify hash/size and fsync complete immutable blob and its directory.
	Finish {
		/// Opaque execution-local handle.
		handle: String,
	},
	/// Commit a checkpoint referencing an already complete verified blob.
	Checkpoint {
		/// Opaque execution-local handle.
		handle: String,
		/// Expected canonical decimal u64 revision.
		revision: String,
		/// Expected previous journal digest.
		head: [u8; 32],
		/// Complete recovery blob.
		recovery: ValidatorRecoveryRef,
	},
	/// Replace all retained recovery witnesses with one complete checkpoint.
	///
	/// The ready execution asserts authenticated recovery coverage of every
	/// retained vote target; consumed-slot guards remain in the host journal.
	/// The host binds this assertion to the exact expected head and durably
	/// rebinds witness references before reclaiming their former blobs. It
	/// never removes signing choices or verifies opaque consensus state itself.
	/// May reference the existing latest blob, requiring no upload capacity.
	Supersede {
		/// Opaque execution-local handle.
		handle: String,
		/// Expected canonical decimal u64 revision covering all retained duties.
		revision: String,
		/// Expected previous journal digest.
		head: [u8; 32],
		/// Complete recovery covering all retained duties at the expected head.
		recovery: ValidatorRecoveryRef,
	},
	/// Begin a separately quota-accounted immutable archive object.
	ArchiveBegin {
		/// Opaque execution-local handle.
		handle: String,
		/// Declared object hash and complete size.
		recovery: ValidatorRecoveryRef,
	},
	/// Append a bounded sequential archive chunk.
	ArchiveChunk {
		/// Opaque execution-local handle.
		handle: String,
		/// Exact next offset.
		offset: u32,
		/// At most 256 KiB.
		bytes: Vec<u8>,
	},
	/// Verify and durably store the lossless archive object.
	ArchiveFinish {
		/// Opaque execution-local handle.
		handle: String,
	},
}

/// Upload progress or durably committed checkpoint.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostValidatorCommitResponse {
	/// Next upload offset expected by the host.
	Upload {
		/// Bytes accepted so far.
		offset: u32,
	},
	/// Immutable complete blob is now referenceable.
	Stored {
		/// Verified recovery reference.
		recovery: ValidatorRecoveryRef,
	},
	/// Durable checkpoint head.
	Committed {
		/// Canonical decimal u64 revision.
		revision: String,
		/// New journal digest.
		head: [u8; 32],
	},
}

/// Compare, persist a typed GRANDPA intent, then sign its exact JAM domain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorSignRequest {
	/// Opaque execution-local handle.
	pub handle: String,
	/// Expected canonical decimal u64 revision.
	pub revision: String,
	/// Expected previous journal digest.
	pub head: [u8; 32],
	/// Supported typed signing domain.
	pub duty: ValidatorGrandpaDuty,
	/// Complete recovery material bound to the intent.
	pub recovery: ValidatorRecoveryRef,
}

/// Signature released only after every required durability barrier succeeds.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorSignResponse {
	/// Deterministic Ed25519 signature over the exact 65-byte JAM domain.
	pub signature: [u8; 64],
	/// Canonical decimal u64 durable revision.
	pub revision: String,
	/// Durable journal digest.
	pub head: [u8; 32],
}

/// Stop admission, drain persistence/signing/transport, reap and release ownership.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorCloseRequest {
	/// Opaque execution-local handle.
	pub handle: String,
}

/// Exact Safrole VRF domain selector; no arbitrary signing message.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ValidatorSealInput {
	/// Ticket-based eligibility and seal domain.
	TicketSeal {
		/// Epoch entropy.
		entropy: [u8; 32],
		/// Ticket attempt.
		attempt: u8,
	},
	/// Fallback slot eligibility and seal domain.
	FallbackSeal {
		/// Epoch entropy.
		entropy: [u8; 32],
	},
}

/// Canonical decoded seal domain retained by the durable helper.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorSealDuty {
	/// Consumed slot; at most one payload can be sealed per slot.
	pub slot: u32,
	/// Author index decoded from the canonical seal payload.
	pub author_index: u16,
	/// Exact VRF domain.
	pub input: ValidatorSealInput,
	/// Blake2b-256 of the canonical encoded seal payload.
	pub payload_hash: [u8; 32],
}

/// Persisted seal intent exposed for complete guest recovery.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorSealIntent {
	/// Canonical decimal durable revision.
	pub revision: String,
	/// Consumed slot and exact domain.
	pub duty: ValidatorSealDuty,
	/// Complete pre-seal recovery material.
	pub recovery: ValidatorRecoveryRef,
}

/// Deterministic eligibility output, not a signature.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorVrfOutputRequest {
	/// Owned execution-local handle.
	pub handle: String,
	/// Canonical encoded parameters matching the enrolled digest, at most 1024 bytes.
	pub parameters: Vec<u8>,
	/// Exact eligibility domain.
	pub input: ValidatorSealInput,
}
/// Bandersnatch eligibility result.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorVrfOutputResponse {
	/// Deterministic VRF output.
	pub output: [u8; 32],
}
/// Ring ticket proof over the guest's consensus-selected waiting set.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorTicketProveRequest {
	/// Owned execution-local handle.
	pub handle: String,
	/// Canonical parameters matching the enrolled digest.
	pub parameters: Vec<u8>,
	/// Ordered waiting-set public keys, at most 1023.
	pub ring: Vec<[u8; 32]>,
	/// Enrolled key's position in the ring.
	pub index: u16,
	/// Epoch entropy.
	pub entropy: [u8; 32],
	/// Bounded ticket attempt under the enrolled parameters.
	pub attempt: u8,
}
/// Anonymous ring-VRF proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorTicketProveResponse {
	/// Canonical ring signature.
	pub proof: [u8; 784],
}
/// Typed `jam_entropy` signature over a ticket id.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorEntropySourceRequest {
	/// Owned execution-local handle.
	pub handle: String,
	/// Canonical parameters matching the enrolled digest.
	pub parameters: Vec<u8>,
	/// Entropy-source ticket id.
	pub ticket_id: [u8; 32],
}
/// Entropy-source VRF signature.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorEntropySourceResponse {
	/// Canonical Bandersnatch signature.
	pub signature: [u8; 96],
}
/// Compare-and-append one durable slot seal before releasing its signature.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorSealRequest {
	/// Owned execution-local handle.
	pub handle: String,
	/// Canonical parameters matching the enrolled digest.
	pub parameters: Vec<u8>,
	/// Expected canonical decimal durable revision.
	pub revision: String,
	/// Expected durable journal digest.
	pub head: [u8; 32],
	/// Exact seal VRF domain.
	pub input: ValidatorSealInput,
	/// Complete recovery material uploaded before seal admission.
	pub recovery: ValidatorRecoveryRef,
	/// Canonical encoded SealPayload, at most 256 KiB; decoded by the helper.
	pub payload: Vec<u8>,
}
/// Released only after intent fsync and durable-head barriers.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorSealResponse {
	/// Canonical Bandersnatch block seal.
	pub signature: [u8; 96],
	/// Canonical decimal durable revision.
	pub revision: String,
	/// Durable journal digest.
	pub head: [u8; 32],
}

/// One canonical audit announcement report.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorAuditReport {
	/// Core index under the enrolled parameters.
	pub core: u16,
	/// Work-report hash.
	pub report: [u8; 32],
}

/// Full-validator signing domains, requiring explicit role-4 consent.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ValidatorFullDuty {
	/// Availability assurance, guarded by anchor.
	Assurance {
		/// Assurance anchor.
		anchor: [u8; 32],
		/// Canonical assurance statement hash.
		statement_hash: [u8; 32],
	},
	/// Work-report guarantee, guarded by report.
	Guarantee {
		/// Work-report hash.
		report: [u8; 32],
	},
	/// Audit announcement, guarded by its exact canonical message hash.
	Announcement {
		/// Audit anchor.
		anchor: [u8; 32],
		/// Audit tranche.
		tranche: u8,
		/// Canonically ordered core/report pairs.
		reports: Vec<ValidatorAuditReport>,
	},
	/// Validity judgement; opposite validity for one report conflicts.
	Judgement {
		/// Work-report hash.
		report: [u8; 32],
		/// Judged validity.
		valid: bool,
	},
}

/// Durable full-duty guard; never enumerated in retained-history pages.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct ValidatorFullIntent {
	/// Canonical decimal durable revision.
	pub revision: String,
	/// Exact typed domain that was signed.
	pub duty: ValidatorFullDuty,
	/// Complete recovery material bound before signing.
	pub recovery: ValidatorRecoveryRef,
}

/// Exact Bandersnatch audit selection domain.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum ValidatorAuditInput {
	/// Initial audit selection.
	Initial {
		/// Audit entropy.
		entropy: [u8; 32],
	},
	/// Subsequent audit selection.
	Subsequent {
		/// Audit entropy.
		entropy: [u8; 32],
		/// Work-report hash.
		report: [u8; 32],
		/// Audit tranche.
		tranche: u8,
	},
}

/// Persist a typed full-duty intent before releasing its Ed25519 signature.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorFullSignRequest {
	/// Owned execution-local handle.
	pub handle: String,
	/// Canonical parameters matching the enrolled digest.
	pub parameters: Vec<u8>,
	/// Expected canonical decimal durable revision.
	pub revision: String,
	/// Expected custody digest.
	pub head: [u8; 32],
	/// Exact typed signing domain.
	pub duty: ValidatorFullDuty,
	/// Complete recovery material bound to this intent.
	pub recovery: ValidatorRecoveryRef,
}

/// Signature released only after the durable full-duty guard is recorded.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorFullSignResponse {
	/// Canonical Ed25519 signature.
	pub signature: [u8; 64],
	/// Canonical decimal durable revision.
	pub revision: String,
	/// Durable custody digest.
	pub head: [u8; 32],
}

/// Typed audit selection proof, never arbitrary-message signing.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorAuditProveRequest {
	/// Owned execution-local handle.
	pub handle: String,
	/// Canonical parameters matching the enrolled digest.
	pub parameters: Vec<u8>,
	/// Exact audit selection domain.
	pub input: ValidatorAuditInput,
}

/// Bandersnatch audit selection proof.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostValidatorAuditProveResponse {
	/// Canonical Bandersnatch signature.
	pub signature: [u8; 96],
}
