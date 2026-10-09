//! Native Chat cryptographic operations; products own delivery and conversation state.

use alloc::{string::String, vec::Vec};
use core::fmt;
use parity_scale_codec::{Decode, Encode};
use zeroize::Zeroize;

use crate::v01::SignedStatement;
use crate::v02::{
    HostNativeChatDevice, HostNativeChatPayment, HostNativeChatPeer, HostNativeChatRichMessage,
};

/// An operation using the calling product's non-exportable Host Chat device.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostProductDeviceChatRequest {
    /// Restore public metadata, pending ciphertext, and private file-transfer progress.
    Initialize,
    /// Independently resolve and bind a peer on the trusted network.
    Bind {
        /// Recipient username; never a caller-provided cryptographic key.
        username: String,
    },
    /// Validate native plaintext and return signed ciphertext for product delivery.
    Prepare {
        /// Recipient root identity previously authenticated by the Host.
        peer_identity: [u8; 32],
        /// Native invitation, identity, or device transport context.
        route: HostNativeChatRoute,
        /// Native request or tagged transport plaintext; outgoing coin secrets are forbidden.
        plaintext: Vec<u8>,
    },
    /// Authenticate and decrypt a complete external native statement, never own output.
    Open {
        /// Complete statement with its native signature proof and authenticated route.
        statement: SignedStatement,
    },
    /// Propose one main-purse payment; this operation requires trusted Host review.
    SendPayment {
        /// Established recipient identity resolved and displayed by the Host.
        peer_identity: [u8; 32],
        /// Stable intent id; retries resume the same immutable payment operation.
        request_id: String,
        /// Amount in the native Coinage cent denomination.
        amount_cents: u64,
    },
    /// Read durable payment status without authorizing another spend.
    PaymentStatus {
        /// Identifier returned by this product's original payment operation.
        operation_id: [u8; 32],
    },
    /// Reconcile payment custody and return opaque statements for product delivery.
    ReconcilePayments,
    /// Select files in trusted Host UI and prepare native rich content without delivery.
    PrepareAttachments {
        /// Established recipient identity authenticated by the Host.
        peer_identity: [u8; 32],
        /// Stable intent id retaining the original files, recipient, and caption.
        request_id: String,
        /// Optional ordinary caption.
        text: Option<String>,
    },
    /// Resume a private download and present or export through trusted Host UI.
    OpenAttachment {
        /// Product-scoped opaque handle, never a ticket, path, or network address.
        attachment_id: [u8; 32],
    },
    /// Acknowledge durable product storage of the legacy view and ordinary ciphertext.
    CommitMigration {
        /// Exact migration snapshot identifier returned by the Host.
        migration_id: [u8; 32],
    },
    /// Continue a Host-authenticated incoming batch without supplying new ciphertext.
    ContinueOpen {
        /// Opaque identifier of the authenticated opening operation.
        open_id: [u8; 32],
        /// Exact continuation cursor returned by the Host.
        cursor: u32,
    },
    /// Continue a bounded snapshot of custody metadata and pending prepared statements.
    ContinueState {
        /// Opaque identifier of the Host-retained public state snapshot.
        state_id: [u8; 32],
        /// Exact continuation cursor returned by the Host.
        cursor: u32,
    },
    /// Read configured raw chain units per native Coinage cent, without spending.
    #[codec(index = 12)]
    PaymentDenomination,
}

impl fmt::Debug for HostProductDeviceChatRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Initialize => f.write_str("Initialize"),
            Self::Bind { username } => f.debug_struct("Bind").field("username", username).finish(),
            Self::Prepare {
                peer_identity,
                route,
                ..
            } => f
                .debug_struct("Prepare")
                .field("peer_identity", peer_identity)
                .field("route", route)
                .field("plaintext", &"[REDACTED]")
                .finish(),
            Self::Open { statement } => f
                .debug_struct("Open")
                .field("statement", statement)
                .finish(),
            Self::SendPayment {
                peer_identity,
                request_id,
                amount_cents,
            } => f
                .debug_struct("SendPayment")
                .field("peer_identity", peer_identity)
                .field("request_id", request_id)
                .field("amount_cents", amount_cents)
                .finish(),
            Self::PaymentStatus { operation_id } => f
                .debug_struct("PaymentStatus")
                .field("operation_id", operation_id)
                .finish(),
            Self::ReconcilePayments => f.write_str("ReconcilePayments"),
            Self::PrepareAttachments {
                peer_identity,
                request_id,
                text,
            } => f
                .debug_struct("PrepareAttachments")
                .field("peer_identity", peer_identity)
                .field("request_id", request_id)
                .field("text", text)
                .finish(),
            Self::OpenAttachment { attachment_id } => f
                .debug_struct("OpenAttachment")
                .field("attachment_id", attachment_id)
                .finish(),
            Self::CommitMigration { migration_id } => f
                .debug_struct("CommitMigration")
                .field("migration_id", migration_id)
                .finish(),
            Self::ContinueOpen { open_id, cursor } => f
                .debug_struct("ContinueOpen")
                .field("open_id", open_id)
                .field("cursor", cursor)
                .finish(),
            Self::ContinueState { state_id, cursor } => f
                .debug_struct("ContinueState")
                .field("state_id", state_id)
                .field("cursor", cursor)
                .finish(),
            Self::PaymentDenomination => f.write_str("PaymentDenomination"),
        }
    }
}

impl Drop for HostProductDeviceChatRequest {
    fn drop(&mut self) {
        if let Self::Prepare { plaintext, .. } = self {
            plaintext.zeroize();
        }
    }
}

/// The authenticated native encryption and statement-routing context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HostNativeChatRoute {
    /// A native invitation addressed to a peer identity.
    Invitation,
    /// Native identity transport used for establishment and device admission.
    Identity,
    /// Native transport between admitted Chat devices.
    Device,
}

/// Peer cryptographic identity independently resolved and bound by the Host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatBinding {
    /// Authenticated peer root identity.
    pub peer_identity: [u8; 32],
    /// Peer identity-level native Chat encryption key.
    pub peer_chat_public_key: [u8; 32],
    /// Identity proof used in the native invitation handshake.
    pub identity_proof: [u8; 32],
}

/// Authenticated incoming native plaintext, potentially containing incoming coin keys.
#[derive(Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatOpened {
    /// Authenticated peer root identity, never the local wallet's own identity.
    pub peer_identity: [u8; 32],
    /// Account that signed the accepted native statement.
    pub sender_account_id: [u8; 32],
    /// Authenticated native context of the incoming statement.
    pub route: HostNativeChatRoute,
    /// Native invitation or tagged request/response plaintext; never outgoing payment memos.
    pub plaintext: Vec<u8>,
}

impl fmt::Debug for HostNativeChatOpened {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HostNativeChatOpened")
            .field("peer_identity", &self.peer_identity)
            .field("sender_account_id", &self.sender_account_id)
            .field("route", &self.route)
            .field("plaintext", &"[REDACTED]")
            .finish()
    }
}

impl Drop for HostNativeChatOpened {
    fn drop(&mut self) {
        self.plaintext.zeroize();
    }
}

/// Continuation metadata for a bounded page of authenticated incoming plaintext.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatOpenPage {
    /// Opaque identifier of the authenticated opening operation.
    pub open_id: [u8; 32],
    /// Cursor identifying this page.
    pub cursor: u32,
    /// Cursor to request next; absent when the authenticated batch is complete.
    pub next_cursor: Option<u32>,
}

/// Continuation metadata for a bounded page of a stable public state snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatStatePage {
    /// Opaque identifier of the Host-retained public state snapshot.
    pub state_id: [u8; 32],
    /// Cursor identifying this page.
    pub cursor: u32,
    /// Cursor to request next; absent when the snapshot is complete.
    pub next_cursor: Option<u32>,
}

/// Signed ciphertext with the native identity required for durable product delivery.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatPrepared {
    /// Exact signed ciphertext to submit again when retrying delivery.
    pub statement: SignedStatement,
    /// Authenticated recipient root identity.
    pub peer_identity: [u8; 32],
    /// Native request identity used to correlate delivery acknowledgments.
    pub request_id: String,
    /// Whether delivery remains pending until a native peer acknowledgment arrives.
    pub requires_ack: bool,
    /// Original product intent id for correlating migrated pending UI, when retained.
    pub client_request_id: Option<String>,
}

/// Native request identity needed to answer a migrated legacy invitation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatMigrationInvitation {
    /// Invitation handle in the accompanying legacy public view.
    pub invitation_id: [u8; 32],
    /// Original native request identity to acknowledge when answering.
    pub request_id: String,
}

/// Cryptographic results and custody metadata; products drive delivery and import.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostProductDeviceChatResponse {
    /// Public local device metadata, including its allowance account.
    pub device: HostNativeChatDevice,
    /// Host-authenticated peer and device metadata.
    pub peers: Vec<HostNativeChatPeer>,
    /// Newly resolved peer binding, when requested.
    pub binding: Option<HostNativeChatBinding>,
    /// Authenticated incoming plaintext; incoming payment import is product-owned.
    pub opened: Vec<HostNativeChatOpened>,
    /// Signed ciphertext for product submission and exact-identity retries.
    pub prepared: Vec<HostNativeChatPrepared>,
    /// Durable public payment status, never outgoing bearer secrets.
    pub payments: Vec<HostNativeChatPayment>,
    /// Rich-content metadata and trusted private-transfer progress.
    pub rich_messages: Vec<HostNativeChatRichMessage>,
    /// Legacy public view retained until the product durably commits migration.
    pub migration: Option<crate::v02::HostProductDeviceChatResponse>,
    /// Snapshot identifier to acknowledge only after persisting its view and ciphertext.
    pub migration_id: Option<[u8; 32]>,
    /// Continuation of an authenticated incoming batch, when opening is paginated.
    pub open_page: Option<HostNativeChatOpenPage>,
    /// Native request identities for invitations in the legacy migration view.
    pub migration_invitations: Vec<HostNativeChatMigrationInvitation>,
    /// Continuation of a bounded public state snapshot, when metadata is paginated.
    pub state_page: Option<HostNativeChatStatePage>,
    /// Configured raw chain units per native Coinage cent, only when requested.
    /// This is positive denomination metadata, never a balance or fiat price.
    pub coinage_cents_unit: Option<u128>,
}
