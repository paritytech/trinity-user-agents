import Foundation
import AsyncExtensions
import SubstrateOperation
import SubstrateSdk

final class MockBlockInfoProvider: BlockInfoProviding {
    let currentHash = Data.random(of: 32)!

    func fetchCurrent() async throws -> BlockNumber { 0 }
    func fetchCurrentHash() async throws -> BlockHashData { currentHash }
    func fetchFinalized() async throws -> BlockNumber { 0 }
    func fetchFinalizedHash() async throws -> BlockHashData { currentHash }
    func fetchBlockHash(_: BlockNumber) async throws -> BlockHashData { currentHash }
    func fetchBlockNumber(byHash _: BlockHashData) async throws -> BlockNumber { 0 }

    func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header> {
        AsyncStream<Block.Header> { $0.finish() }.eraseToAnyAsyncSequence()
    }

    func subscribeNewHeads() -> AnyAsyncSequence<SubstrateSdk.Block.Header> {
        AsyncStream<Block.Header> { $0.finish() }.eraseToAnyAsyncSequence()
    }
}
