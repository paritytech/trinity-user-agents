import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import Operation_iOS
import ChainStore
import os

public protocol DotnsGatewayOperationMaking {
    /// Reads the `AccountNames` entry for each account id in one batched storage query.
    func makeAccountNamesWrapper(
        _ accountIdClosure: @escaping () throws -> [AccountId],
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<[DotnsGatewayPallet.AccountNameWithAccountId]>
}

public final class DotnsGatewayOperationFactory {
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

private extension DotnsGatewayOperationFactory {
    func createAccountNamesWrapper(
        accountIdClosure: @escaping () throws -> [AccountId],
        connection: JSONRPCEngine,
        codingFactoryOperation: BaseOperation<RuntimeCoderFactoryProtocol>,
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<[DotnsGatewayPallet.AccountNameWithAccountId]> {
        let snapshotAccountIdsLock = OSAllocatedUnfairLock(initialState: [AccountId]())

        let fetchWrapper: CompoundOperationWrapper<[StorageResponse<DotnsGatewayPallet.AccountNameRecord>]>

        fetchWrapper = storageRequestFactory.queryItems(
            engine: connection,
            keyParams: {
                let accountIds = try accountIdClosure()
                snapshotAccountIdsLock.withLock {
                    $0 = accountIds
                }
                return accountIds.map { BytesCodable(wrappedValue: $0) }
            },
            factory: { try codingFactoryOperation.extractNoCancellableResultData() },
            storagePath: DotnsGatewayPallet.Storage.accountNames(Data())(),
            at: blockHash
        )

        let mappingOperation = ClosureOperation<[DotnsGatewayPallet.AccountNameWithAccountId]> {
            let responses = try fetchWrapper.targetOperation.extractNoCancellableResultData()

            let accountIds = snapshotAccountIdsLock.withLock { $0 }

            return zip(accountIds, responses).compactMap { accountIdAndResponse in
                guard let record = accountIdAndResponse.1.value else {
                    return nil
                }

                return DotnsGatewayPallet.AccountNameWithAccountId(
                    accountId: accountIdAndResponse.0,
                    record: record
                )
            }
        }

        mappingOperation.addDependency(fetchWrapper.targetOperation)

        return fetchWrapper.insertingTail(operation: mappingOperation)
    }
}

extension DotnsGatewayOperationFactory: DotnsGatewayOperationMaking {
    public func makeAccountNamesWrapper(
        _ accountIdClosure: @escaping () throws -> [AccountId],
        blockHash: BlockHashData?
    ) -> CompoundOperationWrapper<[DotnsGatewayPallet.AccountNameWithAccountId]> {
        do {
            let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
            let runtimeService = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

            let codingFactoryOperation = runtimeService.fetchCoderFactoryOperation()

            let accountNamesWrapper = createAccountNamesWrapper(
                accountIdClosure: accountIdClosure,
                connection: connection,
                codingFactoryOperation: codingFactoryOperation,
                blockHash: blockHash
            )

            accountNamesWrapper.addDependency(operations: [codingFactoryOperation])

            return accountNamesWrapper.insertingHead(operations: [codingFactoryOperation])
        } catch {
            return .createWithError(error)
        }
    }
}
