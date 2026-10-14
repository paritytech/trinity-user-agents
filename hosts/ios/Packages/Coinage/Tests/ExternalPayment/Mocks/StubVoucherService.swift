import BigInt
import KeyDerivation
import ExtrinsicService
import Foundation
import os
@testable import Coinage

/// Serves a configurable tracked-voucher set; `load` is not supported (payments never load vouchers).
final class StubVoucherService: VoucherServiceProtocol, @unchecked Sendable {
    struct NotSupported: Error {}

    private let vouchers = OSAllocatedUnfairLock(initialState: [Voucher]())
    private let states = OSAllocatedUnfairLock(initialState: [CoinageKeyIndex: CoinageAssetState]())

    /// `states` overrides the free default for the listed vouchers (consumed, reserved, …).
    init(vouchers: [Voucher] = [], states: [TrackedVoucher] = []) {
        set(vouchers: vouchers)
        self.states.withLock { table in
            for tracked in states {
                table[tracked.voucher.derivationIndex] = tracked.state
            }
        }
    }

    func set(vouchers: [Voucher]) {
        self.vouchers.withLock { $0 = vouchers }
    }

    func load(
        amount _: BigUInt,
        externalAssetHolder _: any WalletManaging,
        breakdownContext _: DenominationBreakdownContext,
        groupId _: CoinageTxGroupId?
    ) async throws -> [Voucher] {
        throw NotSupported()
    }

    func fetchVouchers(publicKeys: Set<PublicKey>) async throws -> [Voucher] {
        vouchers.withLock { $0 }.filter { publicKeys.contains($0.publicKey) }
    }

    func fetchTracked(derivationIndices: Set<CoinageKeyIndex>) async throws -> [TrackedVoucher] {
        try await fetchAllTracked().filter { derivationIndices.contains($0.voucher.derivationIndex) }
    }

    func fetchAllTracked() async throws -> [TrackedVoucher] {
        let overrides = states.withLock { $0 }
        return vouchers.withLock { $0 }.map {
            TrackedVoucher(
                voucher: $0,
                state: overrides[$0.derivationIndex]
                    ?? CoinageAssetState(handedOff: false, consumerStatus: nil, minterStatus: nil)
            )
        }
    }
}
