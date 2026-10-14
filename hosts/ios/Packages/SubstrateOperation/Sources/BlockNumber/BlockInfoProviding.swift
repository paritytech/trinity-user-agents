import Foundation
import SubstrateSdk
import Operation_iOS
import AsyncExtensions
import StructuredConcurrency
import ChainStore
import SubstrateSdkExt

public protocol BlockInfoProviding {
    func fetchCurrent() async throws -> BlockNumber
    func fetchCurrentHash() async throws -> BlockHashData
    func fetchFinalized() async throws -> BlockNumber
    func fetchFinalizedHash() async throws -> BlockHashData

    func fetchBlockHash(_ blockNumber: BlockNumber) async throws -> BlockHashData
    func fetchBlockNumber(byHash blockHash: BlockHashData) async throws -> BlockNumber

    func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header>
    func subscribeNewHeads() -> AnyAsyncSequence<Block.Header>
}

public enum BlockInfoProviderError: Error {
    /// The header came back but its number is not a hex-encoded block number.
    case invalidBlockNumber(String)
}

public final class BlockInfoProvider: BlockInfoProviding {
    private let blockNumberFactory: BlockNumberOperationFactory
    private let blockHashFactory: BlockHashOperationFactory
    private let chainRegistry: ChainResourceProtocol
    private let chainId: ChainId

    public init(
        chainRegistry: ChainResourceProtocol,
        operationQueue: OperationQueue,
        chainId: ChainId
    ) {
        self.chainRegistry = chainRegistry
        self.chainId = chainId
        blockNumberFactory = BlockNumberOperationFactory(
            chainRegistry: chainRegistry,
            operationQueue: operationQueue
        )
        blockHashFactory = BlockHashOperationFactory()
    }

    public func fetchCurrent() async throws -> BlockNumber {
        try await blockNumberFactory.blockNumberWrapper(
            for: chainId,
            blockType: .best
        )
        .asyncExecute()
    }

    public func fetchCurrentHash() async throws -> BlockHashData {
        try await blockNumberFactory.blockHashWrapper(
            for: chainId,
            blockType: .best
        )
        .asyncExecute()
    }

    public func fetchFinalized() async throws -> BlockNumber {
        try await blockNumberFactory.blockNumberWrapper(
            for: chainId,
            blockType: .finalized
        )
        .asyncExecute()
    }

    public func fetchFinalizedHash() async throws -> BlockHashData {
        try await blockNumberFactory.blockHashWrapper(
            for: chainId,
            blockType: .finalized
        )
        .asyncExecute()
    }

    public func fetchBlockHash(_ blockNumber: BlockNumber) async throws -> BlockHashData {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

        let hashOperation = blockHashFactory.createBlockHashOperation(
            connection: connection,
            for: { blockNumber }
        )

        let mappingOperation = ClosureOperation<BlockHashData> {
            let hashString = try hashOperation.extractNoCancellableResultData()
            return try Data(hexString: hashString)
        }

        mappingOperation.addDependency(hashOperation)

        let wrapper = CompoundOperationWrapper(
            targetOperation: mappingOperation,
            dependencies: [hashOperation]
        )

        return try await wrapper.asyncExecute()
    }

    /// The block's number, read from its header via `chain_getHeader`.
    public func fetchBlockNumber(byHash blockHash: BlockHashData) async throws -> BlockNumber {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

        let operation = JSONRPCListOperation<Block.Header>(
            engine: connection,
            method: RPCMethod.getBlockHeader,
            parameters: [blockHash.toHex(includePrefix: true)]
        )

        let header = try await operation.asyncExecute()

        guard let number = BlockNumber(header.number.withoutHexPrefix(), radix: 16) else {
            throw BlockInfoProviderError.invalidBlockNumber(header.number)
        }

        return number
    }

    public func subscribeFinalizedHeads() -> AnyAsyncSequence<Block.Header> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
            let subscription: AnyAsyncSequence<JSONRPCSubscriptionUpdate<Block.Header>> = connection.asyncSubscribe(
                "chain_subscribeFinalizedHeads",
                unsubscribeMethod: "chain_unsubscribeFinalizedHeads"
            )
            return subscription.map(\.params.result).eraseToAnyAsyncSequence()
        } catch {
            return AsyncFailSequence(error)
                .eraseToAnyAsyncSequence()
        }
    }

    public func subscribeNewHeads() -> AnyAsyncSequence<Block.Header> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
            let subscription: AnyAsyncSequence<JSONRPCSubscriptionUpdate<Block.Header>> = connection.asyncSubscribe(
                "chain_subscribeNewHeads",
                unsubscribeMethod: "chain_unsubscribeNewHeads"
            )
            return subscription.map(\.params.result).eraseToAnyAsyncSequence()
        } catch {
            return AsyncFailSequence(error)
                .eraseToAnyAsyncSequence()
        }
    }
}
