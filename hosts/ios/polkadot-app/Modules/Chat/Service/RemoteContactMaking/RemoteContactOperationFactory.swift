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

        let usernamesByAccountId = try models
            .filter { $0.status != .failed }
            .reduce(into: [AccountId: Set<Data>]()) { usernamesByAccountId, model in
                let accountId = try model.accountId.toAccountId()
                usernamesByAccountId[accountId, default: []].insert(Data(model.username.value.utf8))
            }

        let accountNames = try await dotnsGatewayOperationMaker.accountNames(
            for: Array(usernamesByAccountId.keys),
            blockHash: nil
        )

        // we could have broken records during mapping here
        return accountNames
            .filter { accountName in
                guard let usernames = usernamesByAccountId[accountName.accountId] else {
                    return false
                }

                return accountName.record.hasAnyUsername(in: usernames)
            }
            .compactMap { try? Chat.RemoteContact(accountName: $0) }
    }

    func fetch(by accountId: AccountId) async throws -> Chat.RemoteContact? {
        let accountNames = try await dotnsGatewayOperationMaker.accountNames(for: [accountId], blockHash: nil)

        guard let accountName = accountNames.first else {
            return nil
        }

        return try Chat.RemoteContact(accountName: accountName)
    }
}

private extension DotnsGatewayPallet.AccountNameRecord {
    func hasAnyUsername(in usernames: Set<Data>) -> Bool {
        let onChainUsernames = [lite?.label, full?.label].compactMap { $0 }
        return onChainUsernames.contains { usernames.contains($0) }
    }
}
