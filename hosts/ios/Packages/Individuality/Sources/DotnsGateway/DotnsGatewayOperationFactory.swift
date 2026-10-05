import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import Operation_iOS
import ChainStore

public protocol DotnsGatewayOperationMaking {
    /// Reads the `AccountNames` entry for each account id in one batched storage query.
    func accountNames(
        for accountIds: [AccountId],
        blockHash: BlockHashData?
    ) async throws -> [DotnsGatewayPallet.AccountNameWithAccountId]
}

public final class DotnsGatewayOperationFactory {
    let chainId: ChainId
    let chainRegistry: ChainResourceProtocol
    let storageRequestFactory: StorageRequestFactoryProtocol

    public init(
        chainId: ChainId,
        chainRegistry: ChainResourceProtocol,
        operationQueue: OperationQueue
    ) {
        self.chainId = chainId
        self.chainRegistry = chainRegistry

        storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }
}

extension DotnsGatewayOperationFactory: DotnsGatewayOperationMaking {
    public func accountNames(
        for accountIds: [AccountId],
        blockHash: BlockHashData?
    ) async throws -> [DotnsGatewayPallet.AccountNameWithAccountId] {
        let connection = try chainRegistry.getRpcConnectionOrError(for: chainId)
        let runtimeService = try chainRegistry.getRuntimeCodingServiceOrError(for: chainId)

        let codingFactory = try await runtimeService.fetchCoderFactoryOperation().asyncExecute()

        let responses: [StorageResponse<DotnsGatewayPallet.AccountNameRecord>] = try await storageRequestFactory
            .queryItems(
                engine: connection,
                keyParams: { accountIds.map { BytesCodable(wrappedValue: $0) } },
                factory: { codingFactory },
                storagePath: DotnsGatewayPallet.Storage.accountNames(Data())(),
                at: blockHash
            )
            .asyncExecute()

        return zip(accountIds, responses).compactMap { accountId, response in
            guard let record = response.value else {
                return nil
            }

            return DotnsGatewayPallet.AccountNameWithAccountId(accountId: accountId, record: record)
        }
    }
}
