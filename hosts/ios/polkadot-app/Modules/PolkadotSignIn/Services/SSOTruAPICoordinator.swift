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
    struct Binding {
        let ownKeyId: Chat.Contact.Own
        let serviceFactory: MessageExchageServiceMaking
        let runtime: TrUAPIHostRuntime
        let session: NativeSsoAccountHolderSession
    }

    private let makeBinding: (TrUAPIHostRuntime) throws -> Binding
    private let chainId: ChainModel.Id
    private let chainRegistry: ChainRegistryProtocol
    private let hostsDataProviderFactory: PolkadotSignInHostDataProviderMaking
    private let hostRepository: AnyDataProviderRepository<PolkadotSignInHost>
    private let runtimeProvider: TrUAPIHostRuntimeProviding
    private let rawSender: PolkadotHostMessageSender<SSORawHostMessage>
    private let messageHandler: SSOTrUAPIMessageHandler
    private let logger: LoggerProtocol

    private let state = State()

    init(
        runtimeProvider: TrUAPIHostRuntimeProviding,
        chainId: ChainModel.Id = AppConfig.Chains.chatChain,
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry,
        hostsDataProviderFactory: PolkadotSignInHostDataProviderMaking = PolkadotSignInHostDataProviderFactory(),
        hostRepositoryFactory: PolkadotSignInHostRepositoryMaking = PolkadotSignInHostRepositoryFactory(),
        disconnectApplier: SSORemoteDisconnectApplying = SSORemoteDisconnectApplier(),
        logger: LoggerProtocol = Logger.shared,
        makeBinding: @escaping (TrUAPIHostRuntime) throws -> Binding
    ) {
        self.makeBinding = makeBinding
        self.runtimeProvider = runtimeProvider
        self.chainId = chainId
        self.chainRegistry = chainRegistry
        self.hostsDataProviderFactory = hostsDataProviderFactory
        hostRepository = hostRepositoryFactory.createRepository(forFilter: nil)
        self.logger = logger

        let sender = PolkadotHostMessageSender<SSORawHostMessage>(logger: logger)
        rawSender = sender

        let requestHandler = SSOTrUAPIRequestHandler(
            sender: sender,
            disconnectApplier: disconnectApplier,
            logger: logger
        )

        let processingContext = SSORequestProcessingContext<SSOTrUAPIRequest>(
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
        await state.start { [weak self, runtimeProvider, hostsDataProviderFactory, logger] in
            do {
                let runtime = try await runtimeProvider.sharedRuntime()
                try Task.checkCancellation()
                guard let ownKeyId = try await self?.bindRuntime(runtime) else { return }
                for try await hosts in hostsDataProviderFactory.subscribeHosts() {
                    guard let self else { return }
                    try await self.handleNewHosts(hosts, ownKeyId: ownKeyId)
                }
            } catch is CancellationError {
            } catch {
                guard !Task.isCancelled else { return }
                logger.error("SSOTruAPICoordinator setup error: \(error)")
            }
            await self?.state.finished()
        }
    }

    func throttle() async {
        await state.reset()
    }

    func disconnectHost(byAccountId accountId: Data) async throws {
        guard let (host, disconnectBytes) = await state.disconnectTarget(forAccountId: accountId) else {
            logger.warning("No host found for accountId \(accountId.toHex())")
            return
        }

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

        guard let entry = await state.peer(matching: peer) else {
            logger.warning("Missing active host for peer \(peer.accountId.toHex())")
            return
        }

        logger.info("Will handle \(messages.count) raw message(s) for host \(entry.host.name)")

        await messageHandler.handleMessages(
            messages.map(\.message),
            from: entry.host,
            service: entry.service
        )
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
    func bindRuntime(_ runtime: TrUAPIHostRuntime) async throws -> Chat.Contact.Own {
        let binding = try makeBinding(runtime)
        let connection = try chainRegistry.getConnectionOrError(for: chainId)
        let service = try binding.serviceFactory.makeService(
            statementStoreConnection: StatementStoreConnection(
                connection: connection,
                retryMatcher: StatementSubmitErrorMatcher.retryWhenTimeoutOrNoAllowance(),
                logger: logger
            ),
            delegate: AnyPeerSessionDelegate(self)
        )
        await rawSender.setExchangeService(service)
        try await state.bind(binding, exchangeService: service)
        try Task.checkCancellation()
        return binding.ownKeyId
    }

    func handleNewHosts(_ hosts: [PolkadotSignInHost], ownKeyId: Chat.Contact.Own) async throws {
        var requests = Set<MessageExchange.SessionRequest>()

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
        }

        logger.debug("Setting \(requests.count) host(s) to TrUAPI exchange service")
        try await state.setHosts(hosts)
        await state.updateSessionRequests(requests)
    }
}

// MARK: - State

extension SSOTruAPICoordinator {
    actor State {
        struct Peer {
            let host: PolkadotSignInHost
            let service: NativeSsoAccountHolderService
        }

        private var binding: Binding?
        private var exchangeService: AnyMessageExchangeService<OpaqueSSORawHostMessage>?
        private var peersByAccountId = [Data: Peer]()
        private var hostSubscriptionTask: Task<Void, Never>?

        func start(_ operation: @escaping @Sendable () async -> Void) {
            guard hostSubscriptionTask == nil else { return }
            hostSubscriptionTask = Task { await operation() }
        }

        func finished() {
            if !Task.isCancelled { hostSubscriptionTask = nil }
        }

        func disconnectTarget(forAccountId accountId: Data) -> (PolkadotSignInHost, Data)? {
            guard let host = peersByAccountId[accountId]?.host, let binding else { return nil }
            return (host, binding.runtime.prepareDisconnectRequest())
        }

        func peer(matching peer: MessageExchange.Peer) -> Peer? {
            guard let entry = peersByAccountId[peer.accountId], entry.host.publicKey == peer.publicKey else {
                return nil
            }
            return entry
        }

        func bind(_ binding: Binding, exchangeService: AnyMessageExchangeService<OpaqueSSORawHostMessage>) throws {
            try Task.checkCancellation()
            self.binding = binding
            self.exchangeService = exchangeService
        }

        func setHosts(_ hosts: [PolkadotSignInHost]) throws {
            try Task.checkCancellation()
            guard let binding else { throw TrUAPIRuntimeConfigError.walletLocked }
            var peers = [Data: Peer]()
            for host in hosts {
                let service: NativeSsoAccountHolderService =
                    if let existing = peersByAccountId[host.accountId],
                    existing.host.publicKey == host.publicKey {
                        existing.service
                    } else {
                        try binding.session.openService()
                    }
                peers[host.accountId] = Peer(host: host, service: service)
            }
            peersByAccountId = peers
        }

        func updateSessionRequests(_ requests: Set<MessageExchange.SessionRequest>) {
            guard !Task.isCancelled else { return }
            exchangeService?.updateSessions(requests)
        }

        func reset() {
            exchangeService?.updateSessions([])
            exchangeService = nil
            binding = nil
            peersByAccountId = [:]
            hostSubscriptionTask?.cancel()
            hostSubscriptionTask = nil
        }
    }
}
