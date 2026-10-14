import Foundation
import Foundation_iOS
import SubstrateSdk
import SubstrateStorageQuery
import Operation_iOS
import ChainStore

public protocol ResourcesPalletOperationMaking {
    func makeConsumerWrapperByUsername(
        _ usernameClosure: @escaping () throws -> Data,
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<ResourcesPallet.ConsumerWithAccountId?>

    func makeConsumerWrapperByAccountId(
        _ accountIdClosure: @escaping () throws -> [AccountId],
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<[ResourcesPallet.ConsumerWithAccountId]>
}

enum ResourcesPalletOperationMakerError: Error {
    case invalidUsernameBytes(String)
}

public final class ResourcesPalletOperationFactory {
    let chainId: ChainId
    let chainRegistry: ChainResourceProtocol
    let storageRequestFactory: StorageRequestFactoryProtocol
    let operationQueue: OperationQueue

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        operationQueue: OperationQueue
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.operationQueue = operationQueue

        storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }
}

private extension ResourcesPalletOperationFactory {
    func createConsumerInfoWrapper(
        accountIdClosure: @escaping () throws -> [AccountId],
        connection: JSONRPCEngine,
        codingFactoryOperation: BaseOperation<RuntimeCoderFactoryProtocol>,
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<[ResourcesPallet.ConsumerWithAccountId]> {
        let fetchWrapper: CompoundOperationWrapper<[StorageResponse<ResourcesPallet.ConsumerInfo>]>

        fetchWrapper = storageRequestFactory.queryItems(
            engine: connection,
            keyParams: {
                let accountIds = try accountIdClosure()
                return accountIds.map { BytesCodable(wrappedValue: $0) }
            },
            factory: { try codingFactoryOperation.extractNoCancellableResultData() },
            storagePath: ResourcesPallet.consumers,
            at: blockHash
        )

        let mappingOperation = ClosureOperation<[ResourcesPallet.ConsumerWithAccountId]> {
            let accountIds = try accountIdClosure()

            let responses = try fetchWrapper.targetOperation.extractNoCancellableResultData()

            return zip(accountIds, responses).compactMap { accountIdAndResponse in
                guard let consumer = accountIdAndResponse.1.value else {
                    return nil
                }

                return ResourcesPallet.ConsumerWithAccountId(
                    accountId: accountIdAndResponse.0,
                    info: consumer
                )
            }
        }

        mappingOperation.addDependency(fetchWrapper.targetOperation)

        return fetchWrapper.insertingTail(operation: mappingOperation)
    }

    func createUsernameOwnerInfoWrapper(
        usernameClosure: @escaping () throws -> Data,
        connection: JSONRPCEngine,
        codingFactoryOperation: BaseOperation<RuntimeCoderFactoryProtocol>,
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<AccountId?> {
        let fetchWrapper: CompoundOperationWrapper<[StorageResponse<BytesCodable>]>

        fetchWrapper = storageRequestFactory.queryItems(
            engine: connection,
            keyParams: {
                let username = try usernameClosure()
                return [BytesCodable(wrappedValue: username)]
            },
            factory: { try codingFactoryOperation.extractNoCancellableResultData() },
            storagePath: ResourcesPallet.usernameOwnerOf,
            at: blockHash
        )

        let mapOperation = ClosureOperation<AccountId?> {
            try fetchWrapper.targetOperation.extractNoCancellableResultData().first?.value?.wrappedValue
        }

        mapOperation.addDependency(fetchWrapper.targetOperation)

        return fetchWrapper.insertingTail(operation: mapOperation)
    }
}

extension ResourcesPalletOperationFactory: ResourcesPalletOperationMaking {
    public func makeConsumerWrapperByUsername(
        _ usernameClosure: @escaping () throws -> Data,
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<ResourcesPallet.ConsumerWithAccountId?> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
            let runtimeService = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

            let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()

            let accountIdByUsernameWrapper = createUsernameOwnerInfoWrapper(
                usernameClosure: usernameClosure,
                connection: connection,
                codingFactoryOperation: codingFactoryOperation,
                blockHash: blockHash
            )

            accountIdByUsernameWrapper.addDependency(operations: [codingFactoryOperation])

            let consumerWrapper = OperationCombiningService<
                ResourcesPallet.ConsumerWithAccountId?
            >.compoundNonOptionalWrapper(
                operationQueue: operationQueue
            ) {
                let optAccountId = try accountIdByUsernameWrapper.targetOperation.extractNoCancellableResultData()

                guard let accountId = optAccountId else {
                    return .createWithResult(nil)
                }

                let fetchWrapper = self.createConsumerInfoWrapper(
                    accountIdClosure: { [accountId] },
                    connection: connection,
                    codingFactoryOperation: codingFactoryOperation,
                    blockHash: blockHash
                )

                let mapOperation = ClosureOperation<ResourcesPallet.ConsumerWithAccountId?> {
                    try fetchWrapper.targetOperation.extractNoCancellableResultData().first
                }

                mapOperation.addDependency(fetchWrapper.targetOperation)

                return fetchWrapper.insertingTail(operation: mapOperation)
            }

            consumerWrapper.addDependency(wrapper: accountIdByUsernameWrapper)

            return consumerWrapper
                .insertingHead(operations: accountIdByUsernameWrapper.allOperations)
                .insertingHead(operations: [codingFactoryOperation])
        } catch {
            return .createWithError(error)
        }
    }

    public func makeConsumerWrapperByAccountId(
        _ accountIdClosure: @escaping () throws -> [AccountId],
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<[ResourcesPallet.ConsumerWithAccountId]> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
            let runtimeService = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

            let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()

            let consumerWrapper = createConsumerInfoWrapper(
                accountIdClosure: accountIdClosure,
                connection: connection,
                codingFactoryOperation: codingFactoryOperation,
                blockHash: blockHash
            )

            consumerWrapper.addDependency(operations: [codingFactoryOperation])

            return consumerWrapper.insertingHead(operations: [codingFactoryOperation])
        } catch {
            return .createWithError(error)
        }
    }
}
