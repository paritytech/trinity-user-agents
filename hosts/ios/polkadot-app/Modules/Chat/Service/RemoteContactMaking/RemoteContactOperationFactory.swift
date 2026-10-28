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

        let modelsByAccountId = try Dictionary(grouping: models.filter { $0.status != .failed }) {
            try $0.accountId.toAccountId()
        }

        let accountNames = try await dotnsGatewayOperationMaker.accountNames(
            for: Array(modelsByAccountId.keys),
            blockHash: nil
        )

        // we could have broken records during mapping here
        return accountNames.compactMap { accountName in
            guard
                let models = modelsByAccountId[accountName.accountId],
                let label = accountName.record.listedLabel(
                    among: Self.labels(of: models),
                    assigned: Self.labels(of: models.filter { $0.status == .assigned })
                )
            else {
                return nil
            }

            return try? Chat.RemoteContact(accountName: accountName, usernameLabel: label)
        }
    }

    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        let accountNames = try await dotnsGatewayOperationMaker.accountNames(for: [accountId], blockHash: nil)

        guard let accountName = accountNames.first else {
            return nil
        }

        return try Chat.RemoteContact(accountName: accountName)
    }
}

private extension RemoteContactOperationFactory {
    static func labels(of models: [UsernameResponseModel]) -> Set<Data> {
        Set(models.map { Data($0.username.value.utf8) })
    }
}
