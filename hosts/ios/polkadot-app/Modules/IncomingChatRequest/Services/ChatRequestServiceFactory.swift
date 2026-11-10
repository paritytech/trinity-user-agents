import Foundation
import FoundationExt
import MessageExchangeKit
import StatementStore
import SubstrateSdk
import NovaCrypto
import Keystore_iOS
import SDKLogger
import KeyDerivation
import BackgroundExecution
import ChainRegistry
import Individuality
import IssueMonitoring
import StatementDelivery

protocol ChatRequestServiceMaking {
    func makeDiscoveryService() async throws -> ChatDiscoveryServicing

    func makeIncomingChatRequestService() async throws -> IncomingChatRequestServicing

    func makeChatRequestDeliveryService() async throws -> any OutgoingStatementDelivering<OutgoingChatRequestInput>

    func makeIncomingChatRequestContext() async throws -> IncomingChatRequestCoordinationContext

    func makeOutgoingChatRequestContext() async throws -> OutgoingChatRequestCoordinationContext

    func makeOutgoingChatAcceptanceContext() async -> OutgoingChatAcceptanceCoordinationContext

    func makeChatAcceptanceDeliveryService() async throws
        -> any OutgoingStatementDelivering<OutgoingChatAcceptanceInput>

    func makeIncomingChatAcceptanceService() async throws -> IncomingChatAcceptanceService

    func makeIncomingChatAcceptanceContext() async -> IncomingChatAcceptanceCoordinationContext
}

actor ChatRequestServiceFactory {
    let chatChainId: ChainModel.Id
    let entropyManager: RootEntropyManaging
    let chainRegistry: ChainRegistryProtocol
    let storageFacade: StorageFacadeProtocol
    let discoverySettings: ChatDiscoverySettingsStoring
    let operationQueue: OperationQueue
    let logger: LoggerProtocol
    let remoteContactResolver: RemoteContactResolving
    let notificationAllocator: NotificationStatementAccountAllocating
    let signingFallback: NonPrivateSigningFallbacking
    let backgroundExecutor: BackgroundExecuting
    let issueReporter: IssueReporting

    private var connection: StatementStoreConnecting?
    private var accountSignManager: ChatSignerManager?
    private var accountEncryptionManager: MessageExchangeEncryptionManaging?
    private var diagnostics: ChatRequestDiagnostics?
    private var requestPipeline: ChatStatementPipeline<OutgoingChatRequestInput>?
    private var acceptancePipeline: ChatStatementPipeline<OutgoingChatAcceptanceInput>?

    init(
        remoteContactResolver: RemoteContactResolving,
        notificationAllocator: NotificationStatementAccountAllocating,
        signingFallback: NonPrivateSigningFallbacking,
        backgroundExecutor: BackgroundExecuting,
        issueReporter: IssueReporting,
        chatChainId: ChainModel.Id = AppConfig.Chains.chatChain,
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        discoverySettings: ChatDiscoverySettingsStoring = SettingsManager.shared,
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: SDKLoggerProtocol = Logger.shared
    ) {
        self.remoteContactResolver = remoteContactResolver
        self.notificationAllocator = notificationAllocator
        self.signingFallback = signingFallback
        self.backgroundExecutor = backgroundExecutor
        self.issueReporter = issueReporter
        self.chatChainId = chatChainId
        self.chainRegistry = chainRegistry
        self.entropyManager = entropyManager
        self.storageFacade = storageFacade
        self.discoverySettings = discoverySettings
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension ChatRequestServiceFactory: ChatRequestServiceMaking {
    func makeDiscoveryService() async throws -> ChatDiscoveryServicing {
        let connection = try await makeConnection()
        let signManager = makeAccountSignManager()
        let encryptionManager = makeAccountEncryptionManager()

        let chatRequestFactory = ChatRequestFactory(
            encryptionManager: encryptionManager,
            signManager: signManager,
            remoteContactResolver: remoteContactResolver
        )

        return ChatDiscoveryService(
            signManager: signManager,
            settings: discoverySettings,
            statementStoreConnection: connection,
            chatRequestFactory: chatRequestFactory,
            logger: logger
        )
    }

    func makeIncomingChatRequestService() async throws -> IncomingChatRequestServicing {
        let connection = try await makeConnection()
        let encryptionManager = makeAccountEncryptionManager()
        let signManager = makeAccountSignManager()

        let channelFactory = ChatRequestChannelFactory(
            encryptionManager: encryptionManager,
            signingManager: signManager,
            sessionIdFactory: PeerSessionIdFactory()
        )

        let chatRequestFactory = ChatRequestFactory(
            encryptionManager: encryptionManager,
            signManager: signManager,
            remoteContactResolver: remoteContactResolver
        )

        return IncomingChatRequestService(
            statementStoreConnection: connection,
            channelFactory: channelFactory,
            chatRequestFactory: chatRequestFactory,
            logger: logger
        )
    }

    func makeIncomingChatRequestContext() async throws -> IncomingChatRequestCoordinationContext {
        let encryptionManager = makeAccountEncryptionManager()
        let signManager = makeAccountSignManager()

        return try IncomingChatRequestCoordinationContext(
            discoveryOwnKeyIds: [Chat.Contact.Own.main()],
            matchOwnKeyIds: Chat.Contact.Own.allPossibleIds(),
            requestStoreService: makeRequestStoreService(
                encryptionManager: encryptionManager,
                signManager: signManager
            ),
            contactsStorageService: ContactsLocalStorageService(),
            remoteContactResolver: remoteContactResolver
        )
    }

    func makeOutgoingChatRequestContext() async throws -> OutgoingChatRequestCoordinationContext {
        OutgoingChatRequestCoordinationContext(logger: logger)
    }

    func makeChatRequestDeliveryService() async throws -> any OutgoingStatementDelivering<OutgoingChatRequestInput> {
        try await makeRequestPipeline().delivery
    }

    func makeOutgoingChatAcceptanceContext() async -> OutgoingChatAcceptanceCoordinationContext {
        OutgoingChatAcceptanceCoordinationContext(logger: logger)
    }

    func makeChatAcceptanceDeliveryService() async throws
        -> any OutgoingStatementDelivering<OutgoingChatAcceptanceInput> {
        try await makeAcceptancePipeline().delivery
    }

    func makeIncomingChatAcceptanceService() async throws -> IncomingChatAcceptanceService {
        try await IncomingChatAcceptanceService(
            encryptionManager: makeAccountEncryptionManager(),
            signerManager: makeAccountSignManager(),
            connection: makeConnection(),
            logger: logger
        )
    }

    func makeIncomingChatAcceptanceContext() async -> IncomingChatAcceptanceCoordinationContext {
        let encryptionManager = makeAccountEncryptionManager()
        let signManager = makeAccountSignManager()

        return IncomingChatAcceptanceCoordinationContext(
            requestStoreService: makeRequestStoreService(
                encryptionManager: encryptionManager,
                signManager: signManager
            ),
            contactsStorageService: ContactsLocalStorageService(),
            logger: logger
        )
    }
}

/// Same cached pipelines as first delivery, so renewal and delivery share one delivery actor per kind.
extension ChatRequestServiceFactory: StatementRenewerProviding {
    func makeChatAcceptanceRenewer() async throws -> OutgoingStatementRenewing {
        try await makeAcceptancePipeline().makeRenewer()
    }

    func makeChatRequestRenewer() async throws -> OutgoingStatementRenewing {
        try await makeRequestPipeline().makeRenewer()
    }
}

/// One delivery service shared by first delivery and renewal, so the two never publish the same statement at once.
private struct ChatStatementPipeline<Input: ChatNotificationStatementInput> {
    let store: any OutgoingStatementStoring<Input>
    let resolver: any StatementSignerResolving<Input>
    let service: any OutgoingStatementServicing<Input>
    let delivery: OutgoingStatementDeliveryService<Input>
    let diagnostics: OutgoingStatementDiagnostics

    // No renewal bound: statements renew until acknowledged, failed or the contact is gone.
    func makeRenewer() -> OutgoingStatementRenewing {
        OutgoingStatementRenewer(
            dependencies: OutgoingStatementRenewalDependencies(
                store: store,
                resolver: resolver,
                service: service,
                delivery: delivery
            ),
            bound: nil,
            dateProvider: NowDateProvider(),
            diagnostics: diagnostics
        )
    }
}

private extension ChatRequestServiceFactory {
    func makeRequestPipeline() async throws -> ChatStatementPipeline<OutgoingChatRequestInput> {
        if let requestPipeline {
            return requestPipeline
        }

        let diagnostics = makeDiagnostics()
        let pipeline = try await makePipeline(
            store: ChatRequestOutgoingStore(storageFacade: storageFacade),
            builder: makeRequestStatementBuilder(),
            postDelivery: nil,
            renewalStarved: diagnostics.issues.renewalStarved,
            diagnostics: diagnostics.statementDiagnostics
        )
        requestPipeline = pipeline

        return pipeline
    }

    func makeAcceptancePipeline() async throws -> ChatStatementPipeline<OutgoingChatAcceptanceInput> {
        if let acceptancePipeline {
            return acceptancePipeline
        }

        let issues = ChatAcceptanceIssueDiagnostics.make(reporter: issueReporter)
        let pipeline = try await makePipeline(
            store: ChatAcceptanceOutgoingStore(storageFacade: storageFacade),
            builder: ChatAcceptanceStatementBuilder(
                encryptionManager: makeAccountEncryptionManager(),
                signerManager: makeAccountSignManager()
            ),
            postDelivery: makeAcceptanceNotifier(),
            renewalStarved: nil,
            diagnostics: issues.statementDiagnostics(logger: logger)
        )
        acceptancePipeline = pipeline

        return pipeline
    }

    func makePipeline<Input: ChatNotificationStatementInput>(
        store: any OutgoingStatementStoring<Input>,
        builder: any OutgoingStatementBuilding<Input>,
        postDelivery: (any OutgoingStatementPostDelivering<Input>)?,
        renewalStarved: IssueDiagnostic?,
        diagnostics: OutgoingStatementDiagnostics
    ) async throws -> ChatStatementPipeline<Input> {
        let resolver = ChatNotificationSignerResolver<Input>(
            allocator: notificationAllocator,
            signerManager: makeAccountSignManager(),
            fallback: signingFallback,
            renewalStarved: renewalStarved,
            logger: logger
        )
        let service = try await OutgoingStatementService(
            builder: builder,
            connection: makeConnection(),
            signerManager: makeAccountSignManager()
        )
        let delivery = OutgoingStatementDeliveryService(
            dependencies: OutgoingStatementDeliveryDependencies(
                store: store,
                resolver: resolver,
                service: service,
                builder: builder,
                postDelivery: postDelivery
            ),
            execution: OutgoingStatementDeliveryExecution(backgroundExecutor: backgroundExecutor),
            diagnostics: diagnostics
        )

        return ChatStatementPipeline(
            store: store,
            resolver: resolver,
            service: service,
            delivery: delivery,
            diagnostics: diagnostics
        )
    }

    func makeRequestStatementBuilder() -> ChatRequestStatementBuilder {
        let encryptionManager = makeAccountEncryptionManager()
        let signManager = makeAccountSignManager()

        return ChatRequestStatementBuilder(
            requestFactory: ChatRequestFactory(
                encryptionManager: encryptionManager,
                signManager: signManager,
                remoteContactResolver: remoteContactResolver
            ),
            channelFactory: ChatRequestChannelFactory(
                encryptionManager: encryptionManager,
                signingManager: signManager,
                sessionIdFactory: PeerSessionIdFactory()
            ),
            priorityFactory: StatementPriorityFactory()
        )
    }

    func makeRequestStoreService(
        encryptionManager: MessageExchangeEncryptionManaging,
        signManager: StatementStoreSignerManaging
    ) -> ChatRequestStoreService {
        ChatRequestStoreService(
            storageFacade: storageFacade,
            pushIdFactory: ChatPushIdFactory(
                encryptionManager: encryptionManager,
                signManager: signManager,
                sessionIdFactory: PeerSessionIdFactory(),
                logger: logger
            ),
            deviceEncryptionKeyManager: DeviceEncryptionKeyManager.shared
        )
    }

    func makeAcceptanceNotifier() -> ChatAcceptanceNotifier {
        let encryptionManager = makeAccountEncryptionManager()

        return ChatAcceptanceNotifier(
            apnsClientService: APNSClientService(
                pushIdFactory: ChatPushIdFactory(
                    encryptionManager: encryptionManager,
                    signManager: makeAccountSignManager(),
                    sessionIdFactory: PeerSessionIdFactory(),
                    logger: logger
                ),
                messageCoder: ChatPushMessageCoder(encryptionManager: encryptionManager),
                payloadBuilder: ChatNotificationPayloadBuilder(logger: logger),
                tokenProvider: JWTTokenManager.shared,
                workQueue: DispatchQueue(label: "io.chat.acceptance.push.queue")
            ),
            notifiedMessageIdRepository: NotifiedMessageIdRepositoryFactory(storageFacade: storageFacade)
                .createRepository(),
            logger: logger
        )
    }

    func makeDiagnostics() -> ChatRequestDiagnostics {
        if let diagnostics {
            return diagnostics
        }

        let diagnostics = ChatRequestDiagnostics(
            logger: logger,
            issues: ChatRequestIssueDiagnostics.make(reporter: issueReporter)
        )
        self.diagnostics = diagnostics

        return diagnostics
    }

    func makeConnection() async throws -> StatementStoreConnecting {
        if let connection {
            return connection
        }

        let chainConnection = try chainRegistry.getConnectionOrError(for: chatChainId)

        let connection = StatementStoreConnection(
            connection: chainConnection,
            retryMatcher: StatementSubmitErrorMatcher.retryWhenTimeoutOrNoAllowance(),
            logger: logger
        )

        self.connection = connection

        return connection
    }

    func makeAccountEncryptionManager() -> MessageExchangeEncryptionManaging {
        if let accountEncryptionManager {
            return accountEncryptionManager
        }

        let manager = ChatEncryptionManager(entropyManager: entropyManager)

        accountEncryptionManager = manager

        return manager
    }

    func makeAccountSignManager() -> ChatSignerManager {
        if let accountSignManager {
            return accountSignManager
        }

        let manager = ChatSignerManager(entropyManager: entropyManager)

        accountSignManager = manager

        return manager
    }
}
