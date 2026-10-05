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
    private let dotnsGatewayOperationMaker: DotnsGatewayOperationMaking
    private let usernameOperationFactory: UsernameOperationFactoryProtocol

    init(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        connectionChainId: ChainModel.Id = AppConfig.Chains.assethubChain,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue
    ) {
        usernameOperationFactory = UsernameOperationFactory(tokenProvider: JWTTokenManager.shared)

        dotnsGatewayOperationMaker = DotnsGatewayOperationFactory(
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

        let searchMapOperation = ClosureOperation<[AccountId]> {
            let models = try searchWrapper.targetOperation.extractNoCancellableResultData()
            return try models
                .filter { $0.status != .failed }
                .map { model in
                    try model.accountId.toAccountId()
                }
                .distinct()
        }

        searchMapOperation.addDependency(searchWrapper.targetOperation)

        let accountNamesWrapper = dotnsGatewayOperationMaker.makeAccountNamesWrapper(
            { try searchMapOperation.extractNoCancellableResultData() },
            blockHash: nil
        )

        accountNamesWrapper.addDependency(operations: [searchMapOperation])

        let mapOperation = ClosureOperation {
            let accountNames = try accountNamesWrapper.targetOperation.extractNoCancellableResultData()

            // we could have broken records during mapping here
            return accountNames.compactMap { accountName in
                try? Chat.RemoteContact(accountName: accountName)
            }
        }

        mapOperation.addDependency(accountNamesWrapper.targetOperation)

        return accountNamesWrapper
            .insertingHead(operations: [searchMapOperation])
            .insertingHead(operations: searchWrapper.allOperations)
            .insertingTail(operation: mapOperation)
    }

    private func fetch(by accountId: AccountId) -> CompoundOperationWrapper<Chat.RemoteContact?> {
        let wrapper = dotnsGatewayOperationMaker.makeAccountNamesWrapper({ [accountId] }, blockHash: nil)

        let mappingOperation = ClosureOperation<Chat.RemoteContact?> {
            guard let accountName = try wrapper.targetOperation.extractNoCancellableResultData().first else {
                return nil
            }

            return try Chat.RemoteContact(accountName: accountName)
        }

        mappingOperation.addDependency(wrapper.targetOperation)

        return wrapper.insertingTail(operation: mappingOperation)
    }

    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        let wrapper: CompoundOperationWrapper<Chat.RemoteContact?> = fetch(by: accountId)
        return try await wrapper.asyncExecute()
    }
}
