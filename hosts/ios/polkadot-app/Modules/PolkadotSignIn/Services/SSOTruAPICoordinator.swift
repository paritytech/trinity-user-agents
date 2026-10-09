import Foundation
import Foundation_iOS
import CommonService
import Individuality
import MessageExchangeKit
import StatementStore
import Operation_iOS
import ChainRegistry
import UIKitExt
import TrUAPIHost
import StructuredConcurrency

// MARK: - Coordinator

final class SSOTruAPICoordinator {
    private let ownKeyId: Chat.Contact.Own
    private let serviceFactory: MessageExchageServiceMaking
    private let chainId: ChainModel.Id
    private let chainRegistry: ChainRegistryProtocol
    private let hostsDataProviderFactory: PolkadotSignInHostDataProviderMaking
    private let hostRepository: AnyDataProviderRepository<PolkadotSignInHost>
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let rawSender: PolkadotHostMessageSender<SSORawHostMessage>
    private let messageHandler: any PolkadotHostMessageHandling<SSORawHostMessage>
    private let logger: LoggerProtocol

    private let state = State()

    init(
        ownKeyId: Chat.Contact.Own,
        serviceFactory: MessageExchageServiceMaking,
        runtimeProvider: TrUAPIHostRuntimeProviding,
        chainId: ChainModel.Id = AppConfig.Chains.chatChain,
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        hostsDataProviderFactory: PolkadotSignInHostDataProviderMaking = PolkadotSignInHostDataProviderFactory(),
        hostRepositoryFactory: PolkadotSignInHostRepositoryMaking = PolkadotSignInHostRepositoryFactory(),
        disconnectApplier: SSORemoteDisconnectApplying = SSORemoteDisconnectApplier(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.ownKeyId = ownKeyId
        self.serviceFactory = serviceFactory
        self.runtimeProvider = runtimeProvider
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.hostsDataProviderFactory = hostsDataProviderFactory
        hostRepository = hostRepositoryFactory.createRepository(forFilter: nil)
        self.logger = logger

        let sender = PolkadotHostMessageSender<SSORawHostMessage>(logger: logger)
        rawSender = sender

        let requestHandler = SSOTrUAPIRequestHandler(
            runtimeProvider: runtimeProvider,
            sender: sender,
            disconnectApplier: disconnectApplier,
            logger: logger
        )

        let processingContext = SSORequestProcessingContext<SSORawHostMessage>(
            handlers: [requestHandler],
            logger: logger
        )

        messageHandler = SSOTrUAPIMessageHandler(
            processingContext: processingContext,
            logger: logger
        )
    }
}

extension SSOTruAPICoordinator: MessageExchangeSignInHostCoordinating {
    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        runtimeProvider.setPresentationView(view)
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
            await rawSender.setExchangeService(service)

            await subscribeToHosts()
        } catch {
            logger.error("SSOTruAPICoordinator setup error: \(error)")
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

        let disconnectBytes = try runtimeProvider.sharedRuntime().prepareDisconnectRequest()

        logger.debug("Posting disconnect request to host \(host.name)")
        try await rawSender.postMessage(SSORawHostMessage(rawBytes: disconnectBytes), to: host)

        logger.debug("Removing host \(host.name)")
        let operation = hostRepository.saveOperation({ [] }, { [host.identifier] })
        try await operation.asyncExecute()

        logger.debug("Disconnected host \(host.name)")
    }
}

extension SSOTruAPICoordinator {
    func handleIncomingMessages(
        _ messages: [OpaqueSSORawHostMessage],
        from peer: MessageExchange.Peer,
        completion: @escaping (MessageExchange.ResponseCode) -> Void
    ) async {
        completion(.success)

        guard let host = await state.host(forAccountId: peer.accountId) else {
            logger.warning("Missing active host for peer \(peer.accountId.toHex())")
            return
        }

        logger.info("Will handle \(messages.count) raw message(s) for host \(host.name)")

        await messageHandler.handleMessages(messages.map(\.message), from: host)
    }

    func handleDidPostMessages(
        _ messages: [OpaqueSSORawHostMessage],
        withError error: Error?
    ) async {
        await rawSender.handleDidPostMessages(messages.map(\.message), withError: error)
    }

    func handleSessionReinitialized(retainedMessageIds: Set<String>) async {
        await rawSender.cancelPendingMessages(excluding: retainedMessageIds)
    }
}

private extension SSOTruAPICoordinator {
    func subscribeToHosts() async {
        let task = Task { [weak self] in
            guard let self else { return }

            do {
                let sequence = hostsDataProviderFactory.subscribeHosts()

                for try await hosts in sequence {
                    await handleNewHosts(hosts)
                }
            } catch {
                logger.error("Host subscription error: \(error)")
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

        logger.debug("Setting \(requests.count) host(s) to TrUAPI exchange service")
        await state.setHostsByAccountId(hostsByAccountId)
        await state.updateSessionRequests(requests)
    }
}

// MARK: - State

extension SSOTruAPICoordinator {
    actor State {
        private var exchangeService: AnyMessageExchangeService<OpaqueSSORawHostMessage>?
        private var hostsByAccountId = [Data: PolkadotSignInHost]()
        private var hostSubscriptionTask: Task<Void, Never>?

        func host(forAccountId accountId: Data) -> PolkadotSignInHost? {
            hostsByAccountId[accountId]
        }

        func setExchangeService(_ value: AnyMessageExchangeService<OpaqueSSORawHostMessage>?) {
            exchangeService = value
        }

        func setHostsByAccountId(_ value: [Data: PolkadotSignInHost]) {
            hostsByAccountId = value
        }

        func setHostSubscriptionTask(_ value: Task<Void, Never>?) {
            hostSubscriptionTask = value
        }

        func updateSessionRequests(_ requests: Set<MessageExchange.SessionRequest>) {
            exchangeService?.updateSessions(requests)
        }

        func reset() {
            exchangeService?.updateSessions([])
            hostsByAccountId = [:]
            hostSubscriptionTask?.cancel()
            hostSubscriptionTask = nil
        }
    }
}
