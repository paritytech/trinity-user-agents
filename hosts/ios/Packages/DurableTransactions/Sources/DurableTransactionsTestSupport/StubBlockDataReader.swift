import AsyncExtensions
import DurableTransactions
import Foundation
import SubstrateOperation
import SubstrateSdk

/// Test stub for the ``BlockOutcomeReading`` that ``BlockBodyScan`` scans with; records all calls and
/// returns configured responses. A hash absent from the map returns `.unreadable`.
public actor StubBlockDataReader: BlockOutcomeReading {
    private let lookups: [Data: BlockLookup]
    public private(set) var reads: [Data] = []

    public init(lookups: [Data: BlockLookup] = [:]) {
        self.lookups = lookups
    }

    public func lookUp(_: Data, at blockHash: Data) async -> BlockLookup {
        reads.append(blockHash)
        return lookups[blockHash] ?? .unreadable
    }
}

/// Test stub for ``BlockInfoProviding`` that returns configured block hashes, and their numbers back by
/// hash. Throws when a block is not in the configured mapping, simulating a failed read.
public final class StubBlockInfoProvider: BlockInfoProviding {
    private let hashes: [UInt32: Data]
    private let numbers: [Data: UInt32]

    public init(hashes: [UInt32: Data] = [:]) {
        self.hashes = hashes
        numbers = Dictionary(hashes.map { ($1, $0) }, uniquingKeysWith: { first, _ in first })
    }

    public func fetchCurrentHash() async throws -> BlockHashData {
        Data()
    }

    public func fetchCurrent() async throws -> BlockNumber {
        BlockNumber(0)
    }

    public func fetchFinalized() async throws -> BlockNumber {
        BlockNumber(0)
    }

    public func fetchFinalizedHash() async throws -> BlockHashData {
        Data()
    }

    public func fetchBlockHash(_ number: BlockNumber) async throws -> BlockHashData {
        guard let hash = hashes[UInt32(number)] else {
            throw BlockFetchError.blockNotFound
        }
        return hash
    }

    public func fetchBlockNumber(byHash blockHash: BlockHashData) async throws -> BlockNumber {
        guard let number = numbers[blockHash] else {
            throw BlockFetchError.blockNotFound
        }
        return BlockNumber(number)
    }

    public func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header> {
        AsyncStream<Block.Header> { _ in }.eraseToAnyAsyncSequence()
    }

    public func subscribeNewHeads() -> AnyAsyncSequence<Block.Header> {
        AsyncStream<Block.Header> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}

private enum BlockFetchError: Error {
    case blockNotFound
}
