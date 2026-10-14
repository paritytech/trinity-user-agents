import AsyncExtensions
import BigInt
import DurableTransactions
import ExtrinsicService
import Foundation
import Individuality
import KeyDerivation
import os
import SubstrateSdk
@testable import Coinage

/// A fixed current installation whose counters hand out consecutive items from a settable start.
final class StubCurrentInstallationStore: CoinageCurrentInstallationStoring, @unchecked Sendable {
    private struct State {
        var current: CoinageInstallationId
        var coinItem: DerivationIndex = 0
        var voucherItem: DerivationIndex = 0
        var error: Error?
        var coinRequests = 0
        var voucherRequests = 0
    }

    private let state: OSAllocatedUnfairLock<State>

    init(current: CoinageInstallationId = .test) {
        state = OSAllocatedUnfairLock(initialState: State(current: current))
    }

    /// The item the next coin allocation receives.
    var coinItem: DerivationIndex {
        get { state.withLock { $0.coinItem } }
        set { state.withLock { $0.coinItem = newValue } }
    }

    /// The item the next voucher allocation receives.
    var voucherItem: DerivationIndex {
        get { state.withLock { $0.voucherItem } }
        set { state.withLock { $0.voucherItem = newValue } }
    }

    var error: Error? {
        get { state.withLock { $0.error } }
        set { state.withLock { $0.error = newValue } }
    }

    var coinRequests: Int { state.withLock { $0.coinRequests } }
    var voucherRequests: Int { state.withLock { $0.voucherRequests } }

    func getOrCreateCurrent() async throws -> CoinageInstallationId {
        try state.withLock { state in
            if let error = state.error { throw error }
            return state.current
        }
    }

    func nextCoinItem() async throws -> DerivationIndex {
        try state.withLock { state in
            if let error = state.error { throw error }
            state.coinRequests += 1
            defer { state.coinItem += 1 }
            return state.coinItem
        }
    }

    func nextVoucherItem() async throws -> DerivationIndex {
        try state.withLock { state in
            if let error = state.error { throw error }
            state.voucherRequests += 1
            defer { state.voucherItem += 1 }
            return state.voucherItem
        }
    }
}
