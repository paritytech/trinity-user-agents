import Foundation
import SubstrateSdk
import AsyncExtensions
import SubstrateOperation

final class BlockInfoProvidingMock: BlockInfoProviding {
    var currentHash: BlockHashData = Data(repeating: 0x01, count: 32)
    var headTicks: Int

    var keepsStreamOpen = false
    var currentHashError: Error?

    private let header: Block.Header

    init(header: Block.Header, headTicks: Int) {
        self.header = header
        self.headTicks = headTicks
    }

    func fetchCurrent() async throws -> BlockNumber { 1 }

    func fetchCurrentHash() async throws -> BlockHashData {
        if let currentHashError { throw currentHashError }
        return currentHash
    }

    func fetchFinalized() async throws -> BlockNumber { 1 }
    func fetchFinalizedHash() async throws -> BlockHashData { currentHash }
    func fetchBlockHash(_: BlockNumber) async throws -> BlockHashData { currentHash }
    func fetchBlockNumber(byHash _: BlockHashData) async throws -> BlockNumber { 1 }

    func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header> { makeHeads() }
    func subscribeNewHeads() -> AnyAsyncSequence<Block.Header> { makeHeads() }

    private func makeHeads() -> AnyAsyncSequence<Block.Header> {
        let header = header
        let ticks = headTicks
        let keepsStreamOpen = keepsStreamOpen

        return AsyncStream<Block.Header> { continuation in
            for _ in 0 ..< ticks {
                continuation.yield(header)
            }
            if !keepsStreamOpen {
                continuation.finish()
            }
        }
        .eraseToAnyAsyncSequence()
    }
}

extension BlockInfoProvidingMock {
    static func makeHeader() throws -> Block.Header {
        let json = """
        {
            "digest": { "logs": [] },
            "extrinsicsRoot": "0x00",
            "number": "0x1",
            "stateRoot": "0x00",
            "parentHash": "0x00"
        }
        """
        return try JSONDecoder().decode(Block.Header.self, from: Data(json.utf8))
    }
}
