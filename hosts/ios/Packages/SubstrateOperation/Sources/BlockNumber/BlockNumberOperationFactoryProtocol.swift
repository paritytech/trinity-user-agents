import Foundation
import BigInt
import Operation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import ChainStore

public enum BlockType {
    case best
    case finalized
}

public protocol BlockNumberOperationFactoryProtocol {
    func blockNumberWrapper(
        for chainId: ChainId,
        blockType: BlockType
    ) -> CompoundOperationWrapper<BlockNumber>

    func blockHashWrapper(
        for chainId: ChainId,
        blockType: BlockType
    ) -> CompoundOperationWrapper<BlockHashData>
}

public extension BlockNumberOperationFactoryProtocol {
    func bestBlockWrapper(for chainId: ChainId) -> CompoundOperationWrapper<BlockNumber> {
        blockNumberWrapper(for: chainId, blockType: .best)
    }
}

public final class BlockNumberOperationFactory {
    let chainRegistry: ChainResourceProtocol
    let requestFactory: StorageRequestFactoryProtocol

    public init(chainRegistry: ChainResourceProtocol, operationQueue: OperationQueue) {
        self.chainRegistry = chainRegistry

        requestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }
}

extension BlockNumberOperationFactory: BlockNumberOperationFactoryProtocol {
    enum BlockNumberOperationError: Error {
        case noValue
        case invalidBlockNumber
    }

    public func blockNumberWrapper(
        for chainId: ChainId,
        blockType: BlockType
    ) -> CompoundOperationWrapper<BlockNumber> {
        switch blockType {
        case .best:
            bestBlockNumberWrapper(for: chainId)
        case .finalized:
            finalizedBlockNumberWrapper(for: chainId)
        }
    }

    public func blockHashWrapper(
        for chainId: ChainId,
        blockType: BlockType
    ) -> CompoundOperationWrapper<BlockHashData> {
        switch blockType {
        case .best:
            bestBlockHashWrapper(for: chainId)
        case .finalized:
            finalizedBlockHashWrapper(for: chainId)
        }
    }
}

extension BlockNumberOperationFactory {
    private func bestBlockHashWrapper(for chainId: ChainId) -> CompoundOperationWrapper<BlockHashData> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

            let blockHashOperation: JSONRPCListOperation<String> = JSONRPCListOperation(
                engine: connection,
                method: RPCMethod.getBlockHash
            )

            let mappingOperation = ClosureOperation<BlockHashData> {
                let hashString = try blockHashOperation.extractNoCancellableResultData()
                return try Data(hexString: hashString)
            }

            mappingOperation.addDependency(blockHashOperation)

            return CompoundOperationWrapper(
                targetOperation: mappingOperation,
                dependencies: [blockHashOperation]
            )
        } catch {
            return .createWithError(error)
        }
    }

    private func finalizedBlockHashWrapper(for chainId: ChainId) -> CompoundOperationWrapper<BlockHashData> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

            let finalizedHashOperation: JSONRPCListOperation<String> = JSONRPCListOperation(
                engine: connection,
                method: RPCMethod.getFinalizedBlockHash
            )

            let mappingOperation = ClosureOperation<BlockHashData> {
                let hashString = try finalizedHashOperation.extractNoCancellableResultData()
                return try Data(hexString: hashString)
            }

            mappingOperation.addDependency(finalizedHashOperation)

            return CompoundOperationWrapper(
                targetOperation: mappingOperation,
                dependencies: [finalizedHashOperation]
            )
        } catch {
            return .createWithError(error)
        }
    }

    private func bestBlockNumberWrapper(for chainId: ChainId) -> CompoundOperationWrapper<BlockNumber> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
            let provider = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

            let codingFactoryOperation = provider.fetchCoderFactoryOperation()

            typealias BlockNumberResponse = StorageResponse<StringScaleMapper<BlockNumber>>
            let wrapper: CompoundOperationWrapper<BlockNumberResponse> =
                requestFactory.queryItem(
                    engine: connection,
                    factory: { try codingFactoryOperation.extractNoCancellableResultData() },
                    storagePath: SystemPallet.blockNumberPath
                )

            wrapper.addDependency(operations: [codingFactoryOperation])

            let mappingOperation = ClosureOperation<BlockNumber> {
                let response = try wrapper.targetOperation.extractNoCancellableResultData()

                guard let blockNumber = response.value?.value else {
                    throw BlockNumberOperationError.noValue
                }

                return blockNumber
            }

            mappingOperation.addDependency(wrapper.targetOperation)

            return wrapper
                .insertingHead(operations: [codingFactoryOperation])
                .insertingTail(operation: mappingOperation)
        } catch {
            return .createWithError(error)
        }
    }

    private func finalizedBlockNumberWrapper(for chainId: ChainId) -> CompoundOperationWrapper<BlockNumber> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)

            let finalizedHashOperation: JSONRPCListOperation<String> = JSONRPCListOperation(
                engine: connection,
                method: RPCMethod.getFinalizedBlockHash
            )

            let finalizedHeaderOperation: JSONRPCListOperation<Block.Header> = JSONRPCListOperation(
                engine: connection,
                method: RPCMethod.getBlockHeader
            )

            finalizedHeaderOperation.configurationBlock = {
                do {
                    let blockHash = try finalizedHashOperation.extractNoCancellableResultData()
                    finalizedHeaderOperation.parameters = [blockHash]
                } catch {
                    finalizedHeaderOperation.result = .failure(error)
                }
            }

            finalizedHeaderOperation.addDependency(finalizedHashOperation)

            let mappingOperation = ClosureOperation<BlockNumber> {
                let header = try finalizedHeaderOperation.extractNoCancellableResultData()

                guard let number = BigUInt.fromHexString(header.number) else {
                    throw BlockNumberOperationError.invalidBlockNumber
                }

                return BlockNumber(number)
            }

            mappingOperation.addDependency(finalizedHeaderOperation)

            return CompoundOperationWrapper(
                targetOperation: mappingOperation,
                dependencies: [finalizedHashOperation, finalizedHeaderOperation]
            )
        } catch {
            return .createWithError(error)
        }
    }
}
