import Foundation
import FoundationExt
import UniqueDevice
import MessageExchangeKit
import SubstrateSdk
import NovaCrypto
import KeyDerivation
import Keystore_iOS
import Operation_iOS
import Products
import Individuality

protocol MessageExchangeCoordinatorMaking {
    func makeChatCoordinator() throws -> MessageExchangeChatCoordinating
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
    private let storageFacade: StorageFacadeProtocol
    private let bulletInManager: AllowanceManaging
    private let statementAccountFunding: StatementAccountFunding
    private let identitySigningFallback: NonPrivateSigningFallbacking
    private let statementStoreMonitor: StatementStoreActivityMonitoring
    private let operationQueue: OperationQueue
    private let logger: LoggerProtocol

    init(
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        bulletInManager: AllowanceManaging,
        statementAccountFunding: StatementAccountFunding,
        identitySigningFallback: NonPrivateSigningFallbacking,
        statementStoreMonitor: StatementStoreActivityMonitoring,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.entropyManager = entropyManager
        self.storageFacade = storageFacade
        self.bulletInManager = bulletInManager
        self.statementAccountFunding = statementAccountFunding
        self.identitySigningFallback = identitySigningFallback
        self.statementStoreMonitor = statementStoreMonitor
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension MessageExchangeCoordinatorFactory: MessageExchangeCoordinatorMaking {
    enum Constants {
        static let maxChatStatementSize = 500 * 1_024
        static let maxSSOStatementSize = 500 * 1_024
        static let maxDeviceSyncStatementSize = 500 * 1_024
    }

    func makeChatCoordinator() throws -> MessageExchangeChatCoordinating {
        let encryptionManager = ChatEncryptionManager(
            entropyManager: entropyManager
        )

        let signerManager = ChatSignerManager(entropyManager: entropyManager)

        let pushIdFactory = ChatPushIdFactory(
            encryptionManager: encryptionManager,
            signManager: signerManager,
            sessionIdFactory: PeerSessionIdFactory(),
            logger: logger
        )

        let tokenProvider = JWTTokenManager.shared
        let deviceKeyManager = DeviceEncryptionKeyManager.shared
        let compactorFactory = ChatMessageCompactorFactory(
            allowanceManager: bulletInManager,
            logger: logger
        )

        return try MessageExchangeChatCoordinator(
            serviceFactory: MessageExchangeServiceFactory(
                messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .multidevice),
                entropyManager: entropyManager,
                deviceEncryptionKeyFactory: MultideviceComponentFactory.makeDeviceEncryptionKeyFactory(
                    deviceEncryptionKeyManager: deviceKeyManager
                ),
                maxStatementSize: Constants.maxChatStatementSize,
                operationQueue: operationQueue,
                logger: logger
            ),
            pushIdFactory: pushIdFactory,
            pushMessageCoder: ChatPushMessageCoder(encryptionManager: encryptionManager),
            notificationPayloadBuilder: ChatNotificationPayloadBuilder(logger: logger),
            messageCompacterFactory: compactorFactory,
            signerResolver: ChatAccountFunding(
                funding: statementAccountFunding,
                signerManager: signerManager,
                identityFallback: identitySigningFallback,
                dateProvider: NowDateProvider(),
                issueDiagnostics: .make(reporter: IssueReportingFacade.shared)
            ),
            tokenProvider: tokenProvider,
            chatContactDataProviderFactory: ChatContactDataProviderFactory(
                repositoryFactory: ChatContactRepositoryFactory(storageFacade: storageFacade),
                operationQueue: operationQueue,
                logger: logger
            ),
            statementStoreMonitor: statementStoreMonitor
        )
    }

    func makeTrUAPIHostCoordinator(
        runtimeProvider: TrUAPIHostRuntimeProviding
    ) throws -> MessageExchangeSignInHostCoordinating {
        try SSOTruAPICoordinator(
            ownKeyId: Chat.Contact.Own.sso(),
            serviceFactory: makeSSOServiceFactory(),
            signerResolver: IdentitySignerResolver(signManager: ChatSignerManager(entropyManager: entropyManager)),
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
            signerResolver: IdentitySignerResolver(signManager: ChatSignerManager(entropyManager: entropyManager)),
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
            maxStatementSize: Constants.maxSSOStatementSize,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
