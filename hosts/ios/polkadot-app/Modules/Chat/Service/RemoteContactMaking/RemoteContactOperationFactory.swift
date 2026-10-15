import Foundation
import MessageExchangeKit
import SubstrateSdk
import Operation_iOS
import SubstrateStorageQuery
import Individuality
import ChainRegistry

protocol RemoteContactOperationMaking: RemoteContactResolving {
    func search(by query: String) -> CompoundOperationWrapper<[Chat.RemoteContact]>
}

enum RemoteContactOperationFactoryError: Error {
    case invalidQuery(String)
}

final class RemoteContactOperationFactory {
    private let resourcesOperationMaker: ResourcesPalletOperationMaking
    private let usernameOperationFactory: UsernameOperationFactoryProtocol

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        connectionChainId: ChainModel.Id = AppConfig.Chains.chatChain,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue
    ) {
        usernameOperationFactory = UsernameOperationFactory(tokenProvider: JWTTokenManager.shared)

        resourcesOperationMaker = ResourcesPalletOperationFactory(
            chainId: connectionChainId,
            chainRegistry: chainRegistry,
            operationQueue: operationQueue
        )
    }
}

extension RemoteContactOperationFactory: RemoteContactOperationMaking {
    func search(by query: String) -> CompoundOperationWrapper<[Chat.RemoteContact]> {
        let searchWrapper = usernameOperationFactory.searchUsernameWrapper(
            for: UsernameRequestModel(prefix: query)
        )

        let searchMapOperation = ClosureOperation<[AccountId: Set<Data>]> {
            let models = try searchWrapper.targetOperation.extractNoCancellableResultData()
            return try models
                .filter { $0.status != .failed }
                .reduce(into: [AccountId: Set<Data>]()) { usernamesByAccountId, model in
                    let accountId = try model.accountId.toAccountId()
                    usernamesByAccountId[accountId, default: []].insert(Data(model.username.value.utf8))
                }
        }

        searchMapOperation.addDependency(searchWrapper.targetOperation)

        let consumerWrapper = resourcesOperationMaker.makeConsumerWrapperByAccountId(
            { try Array(searchMapOperation.extractNoCancellableResultData().keys) },
            blockHash: nil
        )

        consumerWrapper.addDependency(operations: [searchMapOperation])

        let mapOperation = ClosureOperation {
            let backendUsernames = try searchMapOperation.extractNoCancellableResultData()
            let consumers = try consumerWrapper.targetOperation.extractNoCancellableResultData()

            // we could have broken records during mapping here
            return consumers
                .filter { consumer in
                    guard let usernames = backendUsernames[consumer.accountId] else {
                        return false
                    }

                    return consumer.info.hasAnyUsername(in: usernames)
                }
                .compactMap { consumer in
                    try? Chat.RemoteContact(consumer: consumer)
                }
        }

        mapOperation.addDependency(consumerWrapper.targetOperation)

        return consumerWrapper
            .insertingHead(operations: [searchMapOperation])
            .insertingHead(operations: searchWrapper.allOperations)
            .insertingTail(operation: mapOperation)
    }

    private func fetch(by accountId: AccountId) -> CompoundOperationWrapper<Chat.RemoteContact?> {
        let wrapper = resourcesOperationMaker.makeConsumerWrapperByAccountId({ [accountId] }, blockHash: nil)

        let mappingOperation = ClosureOperation<Chat.RemoteContact?> {
            guard let consumer = try wrapper.targetOperation.extractNoCancellableResultData().first else {
                return nil
            }

            return try Chat.RemoteContact(consumer: consumer)
        }

        mappingOperation.addDependency(wrapper.targetOperation)

        return wrapper.insertingTail(operation: mappingOperation)
    }

    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        let wrapper: CompoundOperationWrapper<Chat.RemoteContact?> = fetch(by: accountId)
        return try await wrapper.asyncExecute()
    }
}

private extension ResourcesPallet.ConsumerInfo {
    func hasAnyUsername(in usernames: Set<Data>) -> Bool {
        let onChainUsernames = [liteUsername, fullUsername?.wrappedValue].compactMap { $0 }
        return onChainUsernames.contains { usernames.contains($0) }
    }
}
