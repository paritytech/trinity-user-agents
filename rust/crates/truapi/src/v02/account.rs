//! Host-owned native Chat transport and one-shot main-purse payments.
//!
//! Products exchange public views and ordinary messages, never transport private
//! keys, decrypted payment memos, source coins, or arbitrary identity ciphertext.

use alloc::{string::String, vec::Vec};
use parity_scale_codec::{Decode, Encode};

use crate::v01::{ProductAccountId, SignedStatement};

/// An operation on the calling product's Host-owned native Chat device.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostProductDeviceChatRequest {
    /// Restore the installation's public device and durable conversation state.
    Initialize,
    /// Resolve a username in the configured network and send a fresh invitation.
    Invite {
        /// Recipient username; the Host resolves the identity and encryption key.
        username: String,
        /// Initial ordinary text shown to the recipient.
        text: String,
    },
    /// Authenticate, decrypt and durably process a native statement.
    Receive {
        /// The complete statement, including its native signature proof.
        statement: SignedStatement,
    },
    /// Accept an invitation previously authenticated and retained by the Host.
    AcceptInvitation {
        /// Opaque identifier from the Host's invitation view.
        invitation_id: [u8; 32],
    },
    /// Reject an invitation previously authenticated and retained by the Host.
    RejectInvitation {
        /// Opaque identifier from the Host's invitation view.
        invitation_id: [u8; 32],
    },
    /// Send ordinary messages to an established, Host-authenticated peer roster.
    Send {
        /// Recipient identity, not a guest-supplied device or encryption key.
        peer_identity: [u8; 32],
        /// Stable caller id; reuse with different contents is rejected.
        request_id: String,
        /// Native message encodings. Payment and device-control variants are forbidden.
        messages: Vec<Vec<u8>>,
    },
    /// Propose one main-purse payment; this operation always requires Host review.
    SendPayment {
        /// Established recipient identity, resolved and displayed by the Host.
        peer_identity: [u8; 32],
        /// Stable caller id; retry resumes the same durable payment operation.
        request_id: String,
        /// Amount in the native Coinage cent denomination.
        amount_cents: u64,
    },
    /// Read a payment's durable status without authorizing another spend.
    PaymentStatus {
        /// Identifier returned by this product's original payment operation.
        operation_id: [u8; 32],
    },
    /// Resume durable transport work and return newly available public views.
    Reconcile,
    /// Select immutable files in trusted Host UI and send native rich content.
    SendAttachments {
        /// Established recipient identity authenticated by the Host.
        peer_identity: [u8; 32],
        /// Stable intent id; retries retain the original files, recipient and text.
        request_id: String,
        /// Optional ordinary caption.
        text: Option<String>,
    },
    /// Resume a private download and present or export through trusted Host UI.
    OpenAttachment {
        /// Product-scoped opaque handle, never a ticket, path or network address.
        attachment_id: [u8; 32],
    },
}

/// The Host-owned device's public identity and statement-signing account.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatDevice {
    /// Wallet identity on the configured People chain.
    pub identity_account_id: [u8; 32],
    /// Wallet identity's public X25519 key.
    pub identity_chat_public_key: [u8; 32],
    /// Product account used to obtain a statement-store allowance for this device.
    pub product_account: ProductAccountId,
    /// Public statement signer for the Host-owned device.
    pub account_id: [u8; 32],
    /// Public X25519 key; its secret never leaves the Host.
    pub chat_public_key: [u8; 32],
}

/// Public metadata for one authenticated remote device.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatPeerDevice {
    /// Remote device's statement signer.
    pub account_id: [u8; 32],
    /// Remote device's authenticated public X25519 key.
    pub chat_public_key: [u8; 32],
}

/// Public conversation state; products cannot write this roster back to the Host.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatPeer {
    /// Recipient wallet identity.
    pub identity_account_id: [u8; 32],
    /// Username resolved by the Host, if currently available.
    pub username: Option<String>,
    /// Devices admitted by authenticated native invitation/control messages.
    pub devices: Vec<HostNativeChatPeerDevice>,
    /// Native session topics for subscriptions, not request/response channel hashes.
    pub incoming_channels: Vec<[u8; 32]>,
    /// Whether establishment and legacy-device revocation have been acknowledged.
    pub ready_for_payments: bool,
}

/// An authenticated invitation awaiting the user's Chat decision.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatInvitation {
    /// Host-generated stable invitation identifier.
    pub invitation_id: [u8; 32],
    /// Authenticated sender identity.
    pub peer_identity: [u8; 32],
    /// Host-resolved sender username, when available.
    pub username: Option<String>,
    /// Authenticated native invitation timestamp in milliseconds.
    pub timestamp: u64,
    /// Ordinary initial text, never an embedded payment or control message.
    pub text: String,
}

/// Safe ordinary messages from one authenticated native request.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatMessages {
    /// Authenticated counterparty identity.
    pub peer_identity: [u8; 32],
    /// Whether the peer sent these messages; false also covers our welcome text.
    pub incoming: bool,
    /// Native request identifier, retained for message-delivery correlation.
    pub request_id: String,
    /// Native message encodings after custody-sensitive content is removed.
    pub messages: Vec<Vec<u8>>,
}

/// A peer's native delivery acknowledgment, not a payment-clearing receipt.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatAcknowledgment {
    /// Authenticated acknowledging identity.
    pub peer_identity: [u8; 32],
    /// Acknowledged native request identifier.
    pub request_id: String,
    /// Native response code; zero denotes successful delivery processing.
    pub response_code: u8,
}

/// Payment direction relative to the current wallet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HostNativeChatPaymentDirection {
    /// An explicitly approved debit from the user's main purse.
    Outgoing,
    /// A received memo being claimed into the user's main purse.
    Incoming,
}

/// Durable payment state. Delivery and on-chain clearing are deliberately distinct.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNativeChatPaymentState {
    /// Approved inputs are reserved; required split/unload work is in progress.
    Preparing,
    /// The encrypted memo is durable and transport delivery is being retried.
    Delivering,
    /// The peer acknowledged the memo; settlement has not yet been established.
    Delivered,
    /// Received secrets are durably held while their claim is in progress.
    Claiming,
    /// Some, but not all, of the payment has been observed clearing on chain.
    PartiallyCleared {
        /// Confirmed amount in cents.
        cleared_cents: u64,
    },
    /// The complete payment has been verified at chain finality.
    Cleared,
    /// An ambiguous effect is retained for reconciliation; inputs remain reserved.
    Recovering,
    /// A definitive failure; the Host has reconciled any possible prior effects.
    Failed {
        /// A public failure category, never a raw secret-bearing backend error.
        reason: HostNativeChatPaymentFailure,
    },
}

/// Public, non-secret payment failure categories.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HostNativeChatPaymentFailure {
    /// No transaction or memo was accepted and the operation was cancelled.
    Cancelled,
    /// The wallet could not fund the approved amount and maximum debit.
    InsufficientBalance,
    /// Received funds were already spent somewhere other than this claim.
    AlreadySpent,
    /// The received memo was invalid for the native Coinage protocol.
    InvalidMemo,
    /// A finalized transaction failed without completing the intended payment.
    ChainRejected,
}

/// A product-visible payment card; it contains no spendable memo material.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatPayment {
    /// Durable, product-scoped operation identifier.
    pub operation_id: [u8; 32],
    /// Caller request id for outgoing payments; native request id for incoming ones.
    pub request_id: String,
    /// Native message id used to place the payment in conversation history.
    pub message_id: String,
    /// Native message timestamp in milliseconds.
    pub timestamp: u64,
    /// Counterparty identity authenticated by the Host.
    pub peer_identity: [u8; 32],
    /// Incoming or outgoing relative to the current wallet.
    pub direction: HostNativeChatPaymentDirection,
    /// Exact requested/received value in cents.
    pub amount_cents: u64,
    /// Durable transport/clearing state.
    pub state: HostNativeChatPaymentState,
}

/// Public native media metadata; thumbnails are BlurHash text, not executable images.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
pub enum HostNativeChatAttachmentKind {
    /// A general document or other opaque file.
    File,
    /// An image with native dimensions and an optional UTF-8 BlurHash.
    Image {
        /// Pixel width.
        width: u32,
        /// Pixel height.
        height: u32,
        /// Native UTF-8 BlurHash bytes, never a URL or file payload.
        thumbnail: Option<Vec<u8>>,
    },
    /// A video with native duration and an optional UTF-8 BlurHash.
    Video {
        /// Duration in whole seconds.
        duration_seconds: u32,
        /// Native UTF-8 BlurHash bytes, never a URL or file payload.
        thumbnail: Option<Vec<u8>>,
    },
}

/// Safe attachment description, independent of private transfer credentials.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
#[cfg_attr(feature = "uniffi", derive(uniffi::Record))]
pub struct HostNativeChatAttachmentMetadata {
    /// Validated media type; it does not authorize execution or network loading.
    pub mime_type: String,
    /// Exact native file size, verified against the downloaded root and chunks.
    pub size_bytes: u32,
    /// General file, image or video metadata.
    pub kind: HostNativeChatAttachmentKind,
}

/// Durable transfer progress, distinct from message delivery acknowledgment.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNativeChatAttachmentState {
    /// The Host is securing an immutable selected source.
    Preparing,
    /// Native HOP entries are being uploaded.
    Uploading {
        /// File bytes whose prepared entry was accepted.
        uploaded_bytes: u32,
    },
    /// Verified file bytes are being committed to private local storage.
    Downloading {
        /// Verified bytes durably held by the Host.
        downloaded_bytes: u32,
    },
    /// Complete verified bytes are available through trusted Host presentation.
    Ready,
    /// An interrupted transfer retains its exact credentials and progress for retry.
    Recovering,
}

/// Public attachment handle; only the Host can resolve its private backing.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatAttachment {
    /// Opaque handle scoped to the current wallet, network and calling product.
    pub attachment_id: [u8; 32],
    /// Non-secret native metadata.
    pub metadata: HostNativeChatAttachmentMetadata,
    /// Current durable transfer progress.
    pub state: HostNativeChatAttachmentState,
}

/// Native rich-content timeline operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostNativeChatRichMessageKind {
    /// A new ordinary rich message.
    Message,
    /// A new rich message replying to an earlier message.
    Reply {
        /// Referenced native message id.
        message_id: String,
    },
    /// A replacement of the same author's earlier rich content.
    Edited {
        /// Native id of the message being edited.
        message_id: String,
    },
}

/// Authenticated rich content after private file capabilities have been removed.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostNativeChatRichMessage {
    /// Authenticated conversation identity.
    pub peer_identity: [u8; 32],
    /// Whether the remote peer authored this content.
    pub incoming: bool,
    /// Native request id used for delivery acknowledgment.
    pub request_id: String,
    /// Native id of this message or edit event.
    pub message_id: String,
    /// Native timestamp in milliseconds.
    pub timestamp: u64,
    /// New message, reply or edit; authorship checks still apply to edits.
    pub kind: HostNativeChatRichMessageKind,
    /// Ordinary optional text.
    pub text: Option<String>,
    /// Opaque file handles and safe metadata, never native file references.
    pub attachments: Vec<HostNativeChatAttachment>,
}

/// Public updates from a Host-owned Chat operation.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub struct HostProductDeviceChatResponse {
    /// Public local device metadata, including its allowance account.
    pub device: HostNativeChatDevice,
    /// Current authenticated peers and subscription channels.
    pub peers: Vec<HostNativeChatPeer>,
    /// Invitations still awaiting a user decision.
    pub invitations: Vec<HostNativeChatInvitation>,
    /// Newly processed safe ordinary messages.
    pub messages: Vec<HostNativeChatMessages>,
    /// Newly processed native delivery acknowledgments.
    pub acknowledgments: Vec<HostNativeChatAcknowledgment>,
    /// Current public payment statuses belonging to the calling product.
    pub payments: Vec<HostNativeChatPayment>,
    /// Safe rich-content views and their current private-transfer progress.
    pub rich_messages: Vec<HostNativeChatRichMessage>,
}

/// Failure of a Host-owned Chat operation before a public update is available.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum HostProductDeviceChatError {
    /// There is no current authenticated wallet session.
    NotConnected,
    /// The calling product lacks the required Chat or transport capability.
    AccessNotGranted,
    /// The user declined a Host-mediated review, selection or export.
    UserRejected,
    /// The device still needs its statement-store allowance.
    AllowanceRequired,
    /// The recipient has not completed authenticated establishment/revocation.
    PeerNotReady,
    /// An id was reused with different immutable operation parameters.
    OperationConflict,
    /// The proposed request violates native bounds or message policy.
    InvalidRequest,
    /// An incoming statement failed native authentication or decryption.
    InvalidStatement,
    /// The configured network could not resolve the requested identity.
    RecipientNotFound,
    /// The main purse cannot fund the proposed payment.
    InsufficientBalance,
    /// Durable storage could not safely commit the operation.
    StorageUnavailable,
    /// The configured chain or statement-store service is unavailable.
    NetworkUnavailable,
    /// The requested operation does not belong to the calling product.
    OperationNotFound,
    /// This Host cannot select, recover or present the requested private file.
    AttachmentsUnavailable,
}

impl core::fmt::Display for HostProductDeviceChatError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::NotConnected => "The wallet session is no longer connected",
            Self::AccessNotGranted => "Chat or statement delivery permission is not granted",
            Self::UserRejected => "The request was declined",
            Self::AllowanceRequired => "The Host Chat device needs a statement-store allowance",
            Self::PeerNotReady => "The peer has not completed secure device establishment",
            Self::OperationConflict => "The request id was already used for different contents",
            Self::InvalidRequest => "The Chat request is invalid",
            Self::InvalidStatement => "The native Chat statement could not be authenticated",
            Self::RecipientNotFound => {
                "The recipient could not be resolved on the configured network"
            }
            Self::InsufficientBalance => "The main purse cannot fund this payment",
            Self::StorageUnavailable => "Durable wallet storage is unavailable",
            Self::NetworkUnavailable => "The configured network is unavailable",
            Self::OperationNotFound => "The payment operation does not belong to this product",
            Self::AttachmentsUnavailable => "The attachment is unavailable on this Host",
        })
    }
}
