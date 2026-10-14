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

/// An in-memory ``CoinageInstallationRepositoryProtocol`` holding previous installations only.
final class InMemoryInstallations: CoinageInstallationRepositoryProtocol, @unchecked Sendable {
    private let state = OSAllocatedUnfairLock<[CoinageInstallationId: PreviousInstallation]>(initialState: [:])
    private let order = OSAllocatedUnfairLock<[CoinageInstallationId]>(initialState: [])

    func previous(_ id: CoinageInstallationId) -> PreviousInstallation? {
        state.withLock { $0[id] }
    }

    func addPrevious(_ installations: [CoinageInstallationId]) async throws {
        for installation in installations {
            let added = state.withLock { previous -> Bool in
                guard previous[installation] == nil else { return false }
                previous[installation] = PreviousInstallation(
                    id: installation,
                    coinScanNextIndex: 0,
                    voucherScanNextIndex: 0,
                    initialScanCompleted: false,
                    isUserConfirmedCompletion: false
                )
                return true
            }
            if added { order.withLock { $0.append(installation) } }
        }
    }

    func getPrevious() async throws -> [PreviousInstallation] {
        let ids = order.withLock { $0 }
        return state.withLock { previous in ids.compactMap { previous[$0] } }
    }

    func updateCoinScanNextIndex(_ nextIndex: DerivationIndex, for installation: CoinageInstallationId) async throws {
        update(installation) { $0.changing(coinScanNextIndex: nextIndex) }
    }

    func updateVoucherScanNextIndex(
        _ nextIndex: DerivationIndex,
        for installation: CoinageInstallationId
    ) async throws {
        update(installation) { $0.changing(voucherScanNextIndex: nextIndex) }
    }

    func markInitialScanCompleted(_ installation: CoinageInstallationId) async throws {
        update(installation) { $0.changing(initialScanCompleted: true) }
    }

    func markAllUserConfirmed() async throws {
        state.withLock { previous in
            for (id, installation) in previous {
                previous[id] = installation.changing(isUserConfirmedCompletion: true)
            }
        }
    }

    private func update(
        _ installation: CoinageInstallationId,
        _ change: (PreviousInstallation) -> PreviousInstallation
    ) {
        state.withLock { previous in
            guard let existing = previous[installation] else { return }
            previous[installation] = change(existing)
        }
    }
}
