import Foundation
import MessageExchangeKit
import SubstrateSdk
import Operation_iOS
import SubstrateStorageQuery
import Individuality
import ChainRegistry

protocol RemoteContactOperationMaking: RemoteContactResolving {
    func search(by query: String) async throws -> [Chat.RemoteContact]
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
    func search(by query: String) async throws -> [Chat.RemoteContact] {
        let models = try await usernameOperationFactory
            .searchUsernameWrapper(for: UsernameRequestModel(prefix: query))
            .asyncExecute()

        let accountIds = try models
            .filter { $0.status != .failed }
            .map { try $0.accountId.toAccountId() }
            .distinct()

        let accountNames = try await dotnsGatewayOperationMaker.accountNames(for: accountIds, blockHash: nil)

        // we could have broken records during mapping here
        return accountNames.compactMap { try? Chat.RemoteContact(accountName: $0) }
    }

    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        let accountNames = try await dotnsGatewayOperationMaker.accountNames(for: [accountId], blockHash: nil)

        guard let accountName = accountNames.first else {
            return nil
        }

        return try Chat.RemoteContact(accountName: accountName)
    }
}
