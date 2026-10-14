import Foundation
import AsyncExtensions
import SubstrateSdk
import SubstrateOperation
@testable import polkadot_app

/// An actor because the anchor provider resolves its two block hashes concurrently, so the
/// recorded call list is written from two tasks.
actor MockAnchorBlockInfoProvider: BlockInfoProviding {
    var currentHeight: BlockNumber = 0

    private(set) var requestedHashHeights: [BlockNumber] = []

    /// Distinct, reversible per height so a test can assert which block a timestamp was read at.
    static func hash(for height: BlockNumber) -> BlockHashData {
        withUnsafeBytes(of: height.bigEndian) { Data($0) }
    }

    func setCurrentHeight(_ height: BlockNumber) {
        currentHeight = height
    }

    func fetchCurrent() async throws -> BlockNumber {
        currentHeight
    }

    func fetchBlockHash(_ blockNumber: BlockNumber) async throws -> BlockHashData {
        requestedHashHeights.append(blockNumber)
        return Self.hash(for: blockNumber)
    }

    func fetchCurrentHash() async throws -> BlockHashData {
        Self.hash(for: currentHeight)
    }

    func fetchFinalized() async throws -> BlockNumber {
        currentHeight
    }

    func fetchFinalizedHash() async throws -> BlockHashData {
        Self.hash(for: currentHeight)
    }

    func fetchBlockNumber(byHash _: BlockHashData) async throws -> BlockNumber {
        currentHeight
    }

    nonisolated func subscribeNewHeads() -> AnyAsyncSequence<Block.Header> {
        AsyncStream<Block.Header> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    nonisolated func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header> {
        AsyncStream<Block.Header> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}
