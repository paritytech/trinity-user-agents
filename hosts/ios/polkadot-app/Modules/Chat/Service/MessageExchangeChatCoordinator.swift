import Foundation
import Operation_iOS
import MessageExchangeKit
import StatementStore
import SubstrateSdk
import CommonService
import OperationExt
import ChainRegistry

protocol MessageExchangeChatCoordinating: ApplicationServiceProtocol {
    var inboxService: ChatInboxServicing { get }
    var outboxService: ChatOutboxServicing { get }
}

final class MessageExchangeChatCoordinator {
    let outboxService: ChatOutboxServicing
    let inboxService: ChatInboxServicing

    private let pushService: ChatPushServicing
    private let serviceFactory: MessageExchageServiceMaking
    private let messageCompacterFactory: (any ChatMessageCompactorMaking)?
    private let chainRegistry: ChainRegistryProtocol
    private let chatChainId: ChainModel.Id
    let workQueue: DispatchQueue
    private let operationQueue: OperationQueue
    private let senderDeviceActivator: SenderDeviceActivator
    private let deviceMessageBroadcaster: DeviceMessageBroadcaster
    private let pendingDeviceFanOutProcessor: PendingDeviceFanOutProcessor
    private let messageExchangeModeProvider: MessageExchangeModeProviding
    private let statementStoreMonitor: StatementStoreActivityMonitoring

    private var contactsProvider: StreamableProvider<Chat.Contact>?
    private var contactsByIdentifier = [String: Chat.Contact]()
    private var contactsByAccountId = [AccountId: Chat.Contact]()

    private var exchangeService: AnyMessageExchangeService<Chat.OpaqueMessage>? {
        didSet {
            outboxService.exchangeService = exchangeService
        }
    }

    let logger: LoggerProtocol
    let chatContactDataProviderFactory: ChatContactDataProviderMaking

    init(
        serviceFactory: MessageExchageServiceMaking,
        pushIdFactory: ChatPushIdMaking,
        pushMessageCoder: ChatPushMessageCoding,
        notificationPayloadBuilder: ChatNotificationPayloadBuilding,
        chatRequestStoreService: ChatRequestStoreServicing,
        messageCompacterFactory: (any ChatMessageCompactorMaking)?,
        chatChainId: ChainModel.Id = AppConfig.Chains.chatChain,
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        tokenProvider: JWTTokenProviding,
        apnsTokenObserver: any APNSTokenManaging = APNSTokenProviderFacade.sharedManager,
        messagesStorageService: MessagesLocalStorageServicing = MessagesLocalStorageService(),
        contactsStorageService: ContactsLocalStorageServicing = ContactsLocalStorageService(),
        chatContactDataProviderFactory: ChatContactDataProviderMaking,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        statementStoreMonitor: StatementStoreActivityMonitoring,
        workQueue: DispatchQueue = DispatchQueue(label: "ChatCoordinator.workQueue", qos: .utility),
        operationQueue: OperationQueue = OperationManagerFacade.sharedDefaultQueue,
        logger: LoggerProtocol = Logger.shared
    ) {
        let pushNotificationService = ChatPushService(
            apnsTokenObserver: apnsTokenObserver,
            contactsStorageService: contactsStorageService,
            pushIdFactory: pushIdFactory,
            workQueue: workQueue
        )

        pushService = pushNotificationService

        outboxService = ChatOutboxService(
            messagesStorageService: messagesStorageService,
            contactsStorageService: contactsStorageService,
            apnsClientService: APNSClientService(
                pushIdFactory: pushIdFactory,
                messageCoder: pushMessageCoder,
                payloadBuilder: notificationPayloadBuilder,
                tokenProvider: tokenProvider,
                workQueue: workQueue
            ),
            notifiedMessageIdRepository: NotifiedMessageIdRepositoryFactory().createRepository(),
            chatMessageDataProviderFactory: ChatMessageDataProviderFactory(),
            workQueue: workQueue,
            operationQueue: operationQueue,
            logger: logger
        )

        var processors: [IncomingMessageProcessing] = [
            ChatRequestAcceptProcessor(
                messageExchangeModeProvider: messageExchangeModeProvider,
                requestStoreService: chatRequestStoreService,
                messageStoreService: messagesStorageService,
                contactsStorageService: contactsStorageService,
                logger: logger
            ),
            pushNotificationService
        ]

        let deviceUpdateProcessor = MultideviceComponentFactory.makeDeviceUpdateProcessor(
            contactsStorageService: contactsStorageService,
            messageExchangeModeProvider: messageExchangeModeProvider,
            workQueue: workQueue,
            operationQueue: operationQueue,
            logger: logger
        )
        processors.append(deviceUpdateProcessor)

        inboxService = ChatInboxService(
            incomingMessageProcessor: CompoundIncomingMessageProcessor(
                processors: processors
            ),
            messagesStorageService: messagesStorageService,
            workQueue: workQueue,
            operationQueue: operationQueue,
            logger: logger
        )

        self.messageCompacterFactory = messageCompacterFactory
        self.chatChainId = chatChainId
        self.workQueue = workQueue
        self.chatContactDataProviderFactory = chatContactDataProviderFactory
        self.serviceFactory = serviceFactory
        self.chainRegistry = chainRegistry
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.statementStoreMonitor = statementStoreMonitor
        senderDeviceActivator = MultideviceComponentFactory.makeSenderDeviceActivator(
            contactsStorageService: contactsStorageService,
            chatRequestStoreService: chatRequestStoreService,
            messageExchangeModeProvider: messageExchangeModeProvider,
            logger: logger
        )
        deviceMessageBroadcaster = MultideviceComponentFactory.makeDeviceMessageBroadcaster(
            messageExchangeModeProvider: messageExchangeModeProvider,
            logger: logger
        )
        pendingDeviceFanOutProcessor = MultideviceComponentFactory.makePendingDeviceFanOutProcessor(
            chatContactDataProviderFactory: chatContactDataProviderFactory,
            messageExchangeModeProvider: messageExchangeModeProvider,
            logger: logger
        )
        self.logger = logger
        self.operationQueue = operationQueue
    }
}

extension MessageExchangeChatCoordinator: MessageExchangeChatCoordinating {
    func setup() {
        workQueue.async { [weak self] in
            self?.performSetup()
        }
    }

    func throttle() {
        workQueue.async { [weak self] in
            self?.performThrottling()
        }
    }
}

extension MessageExchangeChatCoordinator: ChatContactDataSubscribing, ChatContactDataHandling, AnyProviderAutoCleaning {
    func subscribeToAllContacts() {
        unsubscribeFromAllContacts()
        contactsProvider = subscribeOnChatContacts(on: workQueue)
    }

    func unsubscribeFromAllContacts() {
        clear(streamableProvider: &contactsProvider)
    }

    func handleChatContacts(result: Result<[DataProviderChange<Chat.Contact>], Error>) {
        switch result {
        case let .success(changes):
            apply(changes: changes)
        case let .failure(error):
            logger.error("contacts provider error: \(error)")
        }
    }
}

// MARK: - Contact updates handling

private extension MessageExchangeChatCoordinator {
    func performSetup() {
        do {
            let connection = try chainRegistry.getConnectionOrError(for: chatChainId)

            let compactorFactory = messageCompacterFactory.map { AnyMessageCompactorFactory($0) }

            let statementStoreConnection = StatementStoreConnection(
                connection: connection,
                retryMatcher: StatementSubmitErrorMatcher.retryWhenTimeoutOrNoAllowance(),
                logger: logger
            )

            exchangeService = try serviceFactory.makeService(
                statementStoreConnection: statementStoreMonitor.observing(statementStoreConnection),
                delegate: AnyPeerSessionDelegate(self),
                compactorFactory: compactorFactory
            )

            subscribeToAllContacts()
            Task { [pendingDeviceFanOutProcessor] in
                await pendingDeviceFanOutProcessor.setup()
            }
        } catch {
            logger.error("Can't complete setup: \(error)")
        }
    }

    func performThrottling() {
        unsubscribeFromAllContacts()

        contactsByIdentifier = [:]
        contactsByAccountId = [:]
        outboxService.setContactsByAccountId([:])
        exchangeService?.updateSessions([])
        Task { [pendingDeviceFanOutProcessor] in
            await pendingDeviceFanOutProcessor.throttle()
        }
    }

    func apply(changes: [DataProviderChange<Chat.Contact>]) {
        var updatedByIdentifier = contactsByIdentifier
        var updatedByAccountId = contactsByAccountId

        for change in changes {
            switch change {
            case let .insert(contact):
                updatedByIdentifier[contact.identifier] = contact
                updatedByAccountId[contact.accountId] = contact
            case let .update(contact):
                updatedByIdentifier[contact.identifier] = contact
                updatedByAccountId[contact.accountId] = contact
            case let .delete(identifier):
                if let value = updatedByIdentifier.removeValue(forKey: identifier) {
                    updatedByAccountId.removeValue(forKey: value.accountId)
                }
            }
        }

        contactsByIdentifier = updatedByIdentifier
        contactsByAccountId = updatedByAccountId

        let activeContacts = contactsByAccountId.filter {
            $0.value.chatRequest == nil && !$0.value.isBlocked
        }

        outboxService.setContactsByAccountId(activeContacts)
        pushService.setContacts(activeContacts)

        guard let exchangeService else {
            logger.error("Missing exchange service")
            assertionFailure()
            return
        }

        var sessionRequests = Set<MessageExchange.SessionRequest>()

        // we don't want to create incoming session until user approves the request
        // also do not include blocked contacts
        let sessionContacts = contactsByAccountId.filter {
            !$0.value.hasIncomingChatRequest && !$0.value.isBlocked
        }

        for (_, contact) in sessionContacts {
            sessionRequests.insert(contact.toMessageExchangeSessionRequest())
        }

        exchangeService.updateSessions(sessionRequests)
    }
}

// MARK: - Session Handling Helpers

extension MessageExchangeChatCoordinator {
    func handleSyncRemotePendingMessages(
        _ messages: [Chat.RemoteMessage],
        for peer: MessageExchange.Peer
    ) {
        outboxService.syncPendingRemoteMessages(messages, for: peer)
    }

    func handleSentMessages(
        _ messages: [Chat.RemoteMessage],
        to peer: MessageExchange.Peer,
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        workQueue.async { [weak self] in
            guard let self else {
                return
            }

            if let contact = contactsByAccountId[peer.accountId] {
                outboxService.handleSentMessages(
                    messages,
                    to: contact,
                    withError: error
                )
            } else {
                logger.warning("Missing contact for sent messages: \(peer.accountId.toHex())")
            }

            // TODO: Move sender device activation to delivered callback once MDS
            // responses are stable.
            senderDeviceActivator.handleSentMessages(
                messages,
                to: peer,
                withError: error
            )
        }
    }

    func handleDeliveredMessages(
        _ messages: [Chat.RemoteMessage],
        to peer: MessageExchange.Peer,
        withError error: MessageExchange.OutgoingMessageError?
    ) {
        workQueue.async { [weak self] in
            guard let self else {
                return
            }

            guard let contact = contactsByAccountId[peer.accountId] else {
                logger.warning("Missing contact for delivered messages: \(peer.accountId.toHex())")
                return
            }

            outboxService.handleDeliveredMessages(
                messages,
                to: contact,
                withError: error
            )
        }
    }

    func handleCompactedMessages(
        compactedMessage: Chat.RemoteMessage,
        originalMessages: [Chat.RemoteMessage],
        for peer: MessageExchange.Peer
    ) {
        outboxService.handleCompactedMessages(
            compactedMessage: compactedMessage,
            originalMessages: originalMessages,
            for: peer
        )
    }

    func handleIncomingMessages(
        _ messages: [Chat.RemoteMessage],
        from peer: MessageExchange.Peer,
        completion: @escaping (MessageExchange.ResponseCode) -> Void
    ) {
        workQueue.async { [weak self] in
            guard let self else {
                return
            }

            guard let contact = contactsByAccountId[peer.accountId] else {
                logger.warning("Missing active contact")
                return
            }

            guard !contact.isBlocked else {
                logger.warning("Ignoring messages from blocked contact")
                return
            }

            inboxService.handleIncomingMessages(
                messages: messages,
                from: contact,
                completion: completion
            )
        }
    }
}
