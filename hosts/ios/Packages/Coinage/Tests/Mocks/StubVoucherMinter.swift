import Foundation
@testable import Coinage

/// Mints unlocated vouchers with sequential indices; `error` makes every mint fail.
actor StubVoucherMinter: VoucherMinting {
    private var nextIndex: CoinageKeyIndex = 500
    private let error: Error?

    init(error: Error? = nil) {
        self.error = error
    }

    func mintVoucher(exponent: Int16) async throws -> Voucher {
        if let error { throw error }
        let index = nextIndex
        nextIndex = nextIndex.next()
        return Voucher(
            exponent: exponent,
            derivationIndex: index,
            allocatedAt: Date(),
            readyAt: Date.distantPast,
            remoteState: .unlocated,
            publicKey: Data(repeating: UInt8(truncatingIfNeeded: index.item), count: 32)
        )
    }
}
