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

/// The current-installation row in memory: created once, counted, and lost on demand the way a dropped
/// database loses it.
final class InMemoryCurrentInstallationRepository: CoinageCurrentInstallationRepositoryProtocol, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<(current: CoinageInstallationId?, reads: Int, error: Error?)>(
        initialState: (nil, 0, nil)
    )

    var current: CoinageInstallationId? { state.withLock { $0.current } }
    var reads: Int { state.withLock { $0.reads } }

    var error: Error? {
        get { state.withLock { $0.error } }
        set { state.withLock { $0.error = newValue } }
    }

    /// The database is gone: the next read creates a new installation.
    func dropRow() {
        state.withLock { $0.current = nil }
    }

    func getOrCreateCurrent(
        newInstallation: @escaping @Sendable () throws -> CoinageInstallationId
    ) async throws -> CoinageInstallationId {
        try state.withLock { state in
            state.reads += 1
            if let error = state.error { throw error }
            if let current = state.current { return current }
            let created = try newInstallation()
            state.current = created
            return created
        }
    }
}
