import Foundation
import UniqueDevice
import MessageExchangeKit
import SubstrateSdk
import NovaCrypto
import KeyDerivation
import Keystore_iOS
import Operation_iOS
import Products
import Individuality

/// Makes the sign-in coordinators, which carry Polkadot sign-in over the statement store. The person chats run in the
/// chat product, which reaches the statement store through TrUAPI.
protocol MessageExchangeCoordinatorMaking {
    func makeTrUAPIHostCoordinator(
        runtimeProvider: TrUAPIHostRuntimeProviding
    ) throws -> MessageExchangeSignInHostCoordinating
    func makeNativeHostCoordinator(
        accountManager: ProductsAccountManaging,
        sponsorFactory: TransactionSponsorMaking
    ) throws -> MessageExchangeSignInHostCoordinating
}

final class MessageExchangeCoordinatorFactory {
    private let entropyManager: RootEntropyManaging
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.entropyManager = entropyManager
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension MessageExchangeCoordinatorFactory: MessageExchangeCoordinatorMaking {
    enum Constants {
        static let maxSSOStatementSize = 500 * 1_024
    }

    func makeTrUAPIHostCoordinator(
        runtimeProvider: TrUAPIHostRuntimeProviding
    ) throws -> MessageExchangeSignInHostCoordinating {
        try SSOTruAPICoordinator(
            ownKeyId: Chat.Contact.Own.sso(),
            serviceFactory: makeSSOServiceFactory(),
            runtimeProvider: runtimeProvider
        )
    }

    func makeNativeHostCoordinator(
        accountManager: ProductsAccountManaging,
        sponsorFactory: TransactionSponsorMaking
    ) throws -> MessageExchangeSignInHostCoordinating {
        try MessageExchangeSignInHostCoordinator(
            ownKeyId: Chat.Contact.Own.sso(),
            serviceFactory: makeSSOServiceFactory(),
            accountManager: accountManager,
            sponsorFactory: sponsorFactory,
            routers: ProductRoutersFacade.sso()
        )
    }
}

private extension MessageExchangeCoordinatorFactory {
    func makeSSOServiceFactory() -> MessageExchageServiceMaking {
        MessageExchangeServiceFactory(
            messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .identity),
            entropyManager: entropyManager,
            deviceEncryptionKeyFactory: nil,
            messageRouteSelector: { _ in .identity },
            maxStatementSize: Constants.maxSSOStatementSize,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
