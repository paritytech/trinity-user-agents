import Foundation
import CommonService
import Individuality
import MessageExchangeKit
import Products
import StatementStore
import Operation_iOS
import ChainRegistry
import UIKitExt

protocol MessageExchangeSignInHostCoordinating: AsyncApplicationServicing, AnyObject {
    func disconnectHost(byAccountId accountId: Data) async throws

    /// Anchors SSO confirmations (signing, personhood prompts) to the given
    /// view; until attached, prompts deliver denial.
    @MainActor func setPresentationView(_ view: ControllerBackedProtocol)
}

final class MessageExchangeSignInHostCoordinator {
    private let ownKeyId: Chat.Contact.Own
    private let serviceFactory: MessageExchageServiceMaking
    private let chainId: ChainModel.Id
    private let chainRegistry: ChainRegistryProtocol
    private let hostsDataProviderFactory: PolkadotSignInHostDataProviderMaking
    private let hostRepository: AnyDataProviderRepository<PolkadotSignInHost>
    private let messageHandler: any PolkadotHostMessageHandling<PolkadotHostRemoteMessage>
    private let messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>
    private let routers: ProductRoutersFacadeProtocol
    private let logger: LoggerProtocol

    private let state = State()

    init(
        ownKeyId: Chat.Contact.Own,
        serviceFactory: MessageExchageServiceMaking,
        accountManager: ProductsAccountManaging,
        sponsorFactory: TransactionSponsorMaking,
        routers: ProductRoutersFacadeProtocol,
        chainId: ChainModel.Id = AppConfig.Chains.chatChain,
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        hostsDataProviderFactory: PolkadotSignInHostDataProviderMaking = PolkadotSignInHostDataProviderFactory(),
        hostRepositoryFactory: PolkadotSignInHostRepositoryMaking = PolkadotSignInHostRepositoryFactory(),
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage> =
            PolkadotHostMessageSender<PolkadotHostRemoteMessage>(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.ownKeyId = ownKeyId
        self.serviceFactory = serviceFactory
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.hostsDataProviderFactory = hostsDataProviderFactory
        hostRepository = hostRepositoryFactory.createRepository(forFilter: nil)
        self.messageSender = messageSender
        self.routers = routers
        self.logger = logger

        let signingHandler = TransactionSigningHandler(
            pgasSponsor: sponsorFactory.makePGasSponsor(),
            chainRegistry: chainRegistry,
            router: routers.productsRouter,
            logger: logger
        )

        let personhoodHandlerFactory = APPersonhoodHandlerFactory(
            chainRegistry: chainRegistry,
            routers: routers,
            logger: logger
        )

        let processingContext = SSORequestProcessingContext(
            handlers: Self.makeHandlers(
                accountManager: accountManager,
                personhoodHandlerFactory: personhoodHandlerFactory,
                messageSender: messageSender,
                signingHandler: signingHandler,
                logger: logger
            ),
            logger: logger
        )

        messageHandler = PolkadotHostMessageHandler(
            processingContext: processingContext,
            logger: logger
        )
    }
}

extension MessageExchangeSignInHostCoordinator: MessageExchangeSignInHostCoordinating {
    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        routers.setPresentationView(view)
    }

    func setup() async {
        do {
            let connection = try chainRegistry.getConnectionOrError(for: chainId)

            let service = try serviceFactory.makeService(
                statementStoreConnection: StatementStoreConnection(
                    connection: connection,
                    retryMatcher: StatementSubmitErrorMatcher.retryWhenTimeoutOrNoAllowance(),
                    logger: logger
                ),
                delegate: AnyPeerSessionDelegate(self)
            )
            await state.setExchangeService(service)
            await messageSender.setExchangeService(service)

            await subscribeToHosts()
        } catch {
            logger.error("Unexpected error: \(error)")
        }
    }

    func throttle() async {
        await state.reset()
    }

    func disconnectHost(byAccountId accountId: Data) async throws {
        guard let host = await state.host(forAccountId: accountId) else {
            logger.warning("No host found for accountId \(accountId.toHex())")
            return
        }
        try await disconnectHost(host)
    }
}

extension MessageExchangeSignInHostCoordinator {
    func handleIncomingMessages(
        _ messages: [PolkadotHostRemoteMessage],
        from peer: MessageExchange.Peer,
        completion: @escaping (MessageExchange.ResponseCode) -> Void
    ) async {
        completion(.success)

        guard let host = await state.host(forAccountId: peer.accountId) else {
            logger.warning("Missing active host")
            return
        }

        logger.info("Will handle messages for host: \(host.name) \(messages.count)")
        await messageHandler.handleMessages(
            messages,
            from: host
        )
    }

    func handleDidPostMessages(
        _ messages: [PolkadotHostRemoteMessage],
        withError error: Error?
    ) async {
        await messageSender.handleDidPostMessages(
            messages,
            withError: error
        )
    }

    func handleSessionReinitialized(retainedMessageIds: Set<String>) async {
        await messageSender.cancelPendingMessages(excluding: retainedMessageIds)
    }
}

private extension MessageExchangeSignInHostCoordinator {
    static func makeHandlers(
        accountManager: ProductsAccountManaging,
        personhoodHandlerFactory: APPersonhoodHandlerMaking,
        messageSender: any PolkadotHostMessageSending<PolkadotHostRemoteMessage>,
        signingHandler: TransactionSigningHandling,
        logger: LoggerProtocol
    ) -> [any SSORequestHandling<PolkadotHostRemoteMessage>] {
        [
            SSODisconnectHandler(logger: logger),
            SSOAliasRequestHandler(
                handlerFactory: personhoodHandlerFactory,
                messageSender: messageSender,
                logger: logger
            ),
            SSOCreateProofHandler(
                handlerFactory: personhoodHandlerFactory,
                messageSender: messageSender,
                logger: logger
            ),
            SSOSignVrfRequestHandler(
                handlerFactory: personhoodHandlerFactory,
                messageSender: messageSender,
                logger: logger
            ),
            SSOResourceAllocationRequestHandler(
                accountManager: accountManager,
                messageSender: messageSender,
                logger: logger
            ),
            SSOProductSubtreeRequestHandler(
                accountManager: accountManager,
                messageSender: messageSender,
                logger: logger
            ),
            SSOSigningRequestHandler(
                messageSender: messageSender,
                signingHandler: signingHandler,
                logger: logger
            ),
            SSOCreateTransactionHandler(
                messageSender: messageSender,
                signingHandler: signingHandler,
                logger: logger
            ),
            SSOSignRawLegacyHandler(
                messageSender: messageSender,
                signingHandler: signingHandler,
                logger: logger
            ),
            SSOCreateTransactionLegacyHandler(
                messageSender: messageSender,
                signingHandler: signingHandler,
                logger: logger
            )
        ]
    }

    func subscribeToHosts() async {
        let task = Task { [weak self] in
            guard let self else {
                return
            }

            do {
                let sequence = hostsDataProviderFactory.subscribeHosts()

                for try await hosts in sequence {
                    await handleNewHosts(hosts)
                }
            } catch {
                logger.error("Error: \(error)")
            }
        }
        await state.setHostSubscriptionTask(task)
    }

    func handleNewHosts(_ hosts: [PolkadotSignInHost]) async {
        var requests = Set<MessageExchange.SessionRequest>()
        var hostsByAccountId = [Data: PolkadotSignInHost]()

        for host in hosts {
            let request = MessageExchange.SessionRequest(
                own: ownKeyId.toMessageExchangeOwn(),
                peer: .init(
                    accountId: host.accountId,
                    publicKey: host.publicKey,
                    pin: nil,
                    devices: []
                )
            )

            requests.insert(request)

            hostsByAccountId[host.accountId] = host
        }

        logger.debug("Setting \(requests.count) host(s) to exchange service")
        await state.setHostsByAccountId(hostsByAccountId)
        await state.updateSessionRequests(requests)
    }

    func disconnectHost(_ host: PolkadotSignInHost) async throws {
        let message = PolkadotHostRemoteMessage(
            messageId: UUID().uuidString,
            versionedContent: .v1(.disconnected)
        )

        logger.debug("Going to post disconnected message \(message.messageId) for host \(host.name)")
        try await messageSender.postMessage(message, to: host)

        logger.debug("Going to remove host \(host.name)")
        let operation = hostRepository.saveOperation({ [] }, { [host.identifier] })
        try await operation.asyncExecute()

        logger.debug("Disconnected host \(host.name)")
    }
}
