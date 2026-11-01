import Foundation
import Products
import TrUAPIHost

enum TrUAPIActionConfirmationRequest: Equatable, Sendable {
    case preimageSubmit(productId: ProductId, size: UInt64)
    case productSubtree(productId: ProductId)
    /// Move NFT items out of a purse the product does not own. A nil owner
    /// is the user's own account.
    case nftPurseDrain(
        productId: ProductId,
        fromPurse: UInt32,
        ownerProductId: ProductId?,
        itemCount: UInt32,
        destination: NftPurseDrainTarget
    )
}

/// Where drained NFT items go.
enum NftPurseDrainTarget: Equatable, Sendable {
    /// Fresh keys of this purse.
    case purse(UInt32)
    /// A key outside every purse, as hex.
    case key(hex: String)
    /// A cheque the product hands to another user.
    case cheque
    /// The keys of a deposited cheque, back to its sender.
    case refund
}

@MainActor
final class TrUAPIActionConfirmationContext {
    nonisolated let request: TrUAPIActionConfirmationRequest

    private var continuation: CheckedContinuation<Bool, Never>?

    init(request: TrUAPIActionConfirmationRequest) {
        self.request = request
    }

    deinit {
        continuation?.resume(returning: false)
    }

    func setContinuation(_ continuation: CheckedContinuation<Bool, Never>) {
        self.continuation = continuation
    }

    func deliver(_ approved: Bool) {
        continuation?.resume(returning: approved)
        continuation = nil
    }
}

/// Unlike action confirmation, upload consent preserves the selected lifetime.
@MainActor
final class TrUAPIPreimageConfirmationContext {
    nonisolated let review: PreimageSubmitReview
    private var continuation: CheckedContinuation<TrUAPIPermissionDecision, Never>?

    init(review: PreimageSubmitReview) {
        self.review = review
    }

    deinit {
        continuation?.resume(returning: .deny)
    }

    func setContinuation(_ continuation: CheckedContinuation<TrUAPIPermissionDecision, Never>) {
        self.continuation = continuation
    }

    func deliver(_ decision: TrUAPIPermissionDecision) {
        continuation?.resume(returning: decision)
        continuation = nil
    }
}
