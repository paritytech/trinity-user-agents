import Foundation

/// The folded outcome of one recycling group, as observed by an external payment.
public enum RecyclingStatus: Equatable, Sendable {
    /// Nothing registered yet, or at least one extrinsic still live.
    case pending
    /// Every extrinsic executed; `vouchers` are the minted recycler vouchers. `finalized` is true
    /// once all of them are final, false while any is only best-block included.
    case allRecycled(vouchers: [TrackedVoucher], finalized: Bool)
    /// At least one extrinsic failed and none is live any more.
    case incomplete
}
