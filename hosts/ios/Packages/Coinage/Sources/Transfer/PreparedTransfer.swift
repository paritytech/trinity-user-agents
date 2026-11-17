import Foundation
import TrUAPIHost

/// An export the core has reserved coins for: the memo carrying their secrets, and the steps that end
/// the reservation.
///
/// ``commit()`` must run before the memo is made durable anywhere, since a committed export is one the
/// wallet no longer counts. A payment whose memo never left calls ``abandon(reclaimId:retryUntil:)``.
public final class PreparedTransfer: @unchecked Sendable {
    public let memo: TransferMemo
    public let exportId: CoinageOperationId

    private let core: CoinageCore
    private let lock = NSLock()
    private var isCommitted = false

    init(memo: TransferMemo, exportId: CoinageOperationId, core: CoinageCore) {
        self.memo = memo
        self.exportId = exportId
        self.core = core
    }

    public func commit() async throws {
        try await core.use { try await $0.commitExport(id: exportId.value) }

        lock.lock()
        isCommitted = true
        lock.unlock()
    }

    /// Ends a payment whose memo never left: an uncommitted export is released, and the coins of a
    /// committed one are taken back under the operation `reclaimId`.
    public func abandon(reclaimId: CoinageOperationId, retryUntil: Date) async throws {
        lock.lock()
        let committed = isCommitted
        lock.unlock()

        guard committed else {
            try await core.use { try await $0.releaseExport(id: exportId.value) }
            return
        }

        _ = try await core.use { coinage in
            try await coinage.reclaim(id: reclaimId.value, retry: .until(retryUntil.unixSeconds), export: exportId.value)
        }
    }
}

extension Date {
    var unixSeconds: UInt64 {
        UInt64(max(timeIntervalSince1970, 0))
    }
}
