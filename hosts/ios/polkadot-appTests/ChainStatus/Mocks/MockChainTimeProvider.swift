import Foundation
import Individuality
import SubstrateSdk
@testable import polkadot_app

enum MockChainTimeProviderError: Error {
    case noTimestampForHash
}

/// Keyed by block hash rather than by call order, so a test states which block carries which
/// chain time and a misdirected read fails instead of silently picking the next value.
actor MockChainTimeProvider: ChainTimeProviding {
    private var secondsByHash: [Data: UInt64] = [:]

    func setSeconds(_ seconds: UInt64, forHeight height: BlockNumber) {
        secondsByHash[MockAnchorBlockInfoProvider.hash(for: height)] = seconds
    }

    func nowSeconds(at blockHash: Data?) async throws -> UInt64 {
        guard let blockHash, let seconds = secondsByHash[blockHash] else {
            throw MockChainTimeProviderError.noTimestampForHash
        }

        return seconds
    }
}
