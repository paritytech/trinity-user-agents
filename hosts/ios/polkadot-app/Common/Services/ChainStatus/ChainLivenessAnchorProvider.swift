import Foundation
import ChainRegistry
import SubstrateSdk
import SubstrateOperation
import SubstrateStorageQuery
import Individuality

struct ChainLivenessAnchor: Equatable {
    let headHeight: BlockNumber
    let chainTimeSpanSeconds: Double
}

protocol ChainLivenessAnchorProviding: Actor {
    func fetchAnchor(for target: ChainConnectionTarget, slotCount: Int) async throws -> ChainLivenessAnchor
}

/// An actor, like its sibling `ChainBlockProvider`: the per-chain providers it holds are not
/// Sendable, and actor isolation is what makes owning them legitimate rather than suppressed.
actor ChainLivenessAnchorProvider: ChainLivenessAnchorProviding {
    private let blockInfoProviders: [ChainConnectionTarget: BlockInfoProviding]
    private let chainTimeProviders: [ChainConnectionTarget: ChainTimeProviding]

    init(
        blockInfoProviders: [ChainConnectionTarget: BlockInfoProviding],
        chainTimeProviders: [ChainConnectionTarget: ChainTimeProviding]
    ) {
        self.blockInfoProviders = blockInfoProviders
        self.chainTimeProviders = chainTimeProviders
    }

    func fetchAnchor(for target: ChainConnectionTarget, slotCount: Int) async throws -> ChainLivenessAnchor {
        guard let blockInfoProvider = blockInfoProviders[target] else {
            throw ChainLivenessAnchorProviderError.blockInfoProviderUnavailable
        }
        guard let chainTimeProvider = chainTimeProviders[target] else {
            throw ChainLivenessAnchorProviderError.chainTimeProviderUnavailable
        }

        // The height is resolved first and both hashes derive from it, so `timeHead` is the
        // time of block `height` itself. Taking the best hash and the best height as separate
        // calls would leave them a block apart, shrinking the span and flattering liveness.
        let height = try await blockInfoProvider.fetchCurrent()

        guard height >= UInt32(slotCount) else {
            throw ChainLivenessAnchorProviderError.chainTooShort
        }

        let headHash = try await blockInfoProvider.fetchBlockHash(height)
        let prevHash = try await blockInfoProvider.fetchBlockHash(height - UInt32(slotCount))

        let timeHead = try await chainTimeProvider.nowSeconds(at: headHash)
        let timePrev = try await chainTimeProvider.nowSeconds(at: prevHash)

        let span = Double(timeHead) - Double(timePrev)
        guard span > 0 else {
            throw ChainLivenessAnchorProviderError.invalidTimeSpan
        }

        return ChainLivenessAnchor(headHeight: height, chainTimeSpanSeconds: span)
    }
}

enum ChainLivenessAnchorProviderError: Error, Equatable {
    case blockInfoProviderUnavailable
    case chainTimeProviderUnavailable
    case chainTooShort
    case invalidTimeSpan
}
