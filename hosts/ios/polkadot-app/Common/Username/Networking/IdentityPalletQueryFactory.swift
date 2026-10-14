import Foundation
import SubstrateSdk
import Operation_iOS
import SubstrateStorageQuery
import Individuality
import ChainRegistry

typealias AccountIdToUsername = [AccountId: Username]

protocol IdentityPalletQueryFactoryProtocol {
    func queryUsernames(
        for accountIds: [AccountId],
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeProviderProtocol
    ) -> CompoundOperationWrapper<AccountIdToUsername>
}

extension IdentityPalletQueryFactoryProtocol {
    func queryUsername(
        for accountId: AccountId,
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeProviderProtocol
    ) -> CompoundOperationWrapper<Username?> {
        let fetchWrapper = queryUsernames(
            for: [accountId],
            connection: connection,
            runtimeProvider: runtimeProvider
        )

        let mappingOperation = ClosureOperation<Username?> {
            let store = try fetchWrapper.targetOperation.extractNoCancellableResultData()
            return store[accountId]
        }

        mappingOperation.addDependency(fetchWrapper.targetOperation)

        return fetchWrapper.insertingTail(operation: mappingOperation)
    }
}

final class IdentityPalletQueryFactory {
    let storageRequestFactory: StorageRequestFactoryProtocol

    init(operationQueue: OperationQueue) {
        storageRequestFactory = StorageRequestFactory(
            remoteFactory: StorageKeyFactory(),
            operationManager: OperationManager(operationQueue: operationQueue)
        )
    }
}

extension IdentityPalletQueryFactory: IdentityPalletQueryFactoryProtocol {
    func queryUsernames(
        for accountIds: [AccountId],
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeProviderProtocol
    ) -> CompoundOperationWrapper<AccountIdToUsername> {
        let wrapper = usernames(
            for: accountIds,
            connection: connection,
            runtimeProvider: runtimeProvider
        )

        let mappingOperation = ClosureOperation<AccountIdToUsername> {
            let responses = try wrapper.targetOperation.extractNoCancellableResultData()

            return zip(accountIds, responses).reduce(into: [AccountId: Username]()) { accum, pair in
                guard let username = pair.1 else {
                    return
                }

                accum[pair.0] = username
            }
        }

        mappingOperation.addDependency(wrapper.targetOperation)
        return wrapper
            .insertingTail(operation: mappingOperation)
    }

    private func usernames(
        for accountIds: [AccountId],
        connection: JSONRPCEngine,
        runtimeProvider: RuntimeProviderProtocol
    ) -> CompoundOperationWrapper<[Username?]> {
        let codingFactoryOperation = runtimeProvider.fetchCoderFactoryOperation()

        let path = ResourcesPallet.Storage.consumers(AccountId.empty)
        let wrapper: CompoundOperationWrapper<[StorageResponse<ResourcesPallet.ConsumerInfo>]> = storageRequestFactory
            .queryItems(
                engine: connection,
                keyParams: { accountIds.map { BytesCodable(wrappedValue: $0) } },
                factory: { try codingFactoryOperation.extractNoCancellableResultData() },
                storagePath: path()
            )
        let toUsername: ClosureOperation<[Username?]> = ClosureOperation {
            let responses = try wrapper.targetOperation.extractNoCancellableResultData()
            return responses.map {
                guard let data = $0.value?.username else {
                    return nil
                }
                return Username(rawData: data)
            }
        }

        wrapper.addDependency(operations: [codingFactoryOperation])
        toUsername.addDependency(wrapper.targetOperation)

        return wrapper
            .insertingHead(operations: [codingFactoryOperation])
            .insertingTail(operation: toUsername)
    }
}
