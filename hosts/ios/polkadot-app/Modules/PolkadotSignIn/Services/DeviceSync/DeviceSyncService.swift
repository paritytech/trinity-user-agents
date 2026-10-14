import Foundation
import CryptoKit
import SubstrateSdk
import StatementStore
import MessageExchangeKit
import AsyncExtensions
import Operation_iOS
import FoundationExt
@preconcurrency import WebRTC

protocol DeviceSyncServicing {
    func setup(configuration: DeviceSyncServiceConfiguration) async
    func throttle() async
}

struct DeviceSyncServiceConfiguration {
    let connection: StatementStoreConnecting
    let signerManager: StatementStoreSignerManaging
    let encryptionManager: MessageExchangeEncryptionManaging
    let ownSignKeyId: String
    let ownEncryptionKeyId: String
}

actor DeviceSyncService {
    typealias PeerEngineFactory = (
        Data,
        any DeviceSyncPeerEngineContextMaking
    ) -> any DeviceSyncPeerEngining

    private let ownStatementAccountId: Data
    private let configFactory: WebRTCConfigMaking
    private let peerConnectionFactory: RTCPeerConnectionFactory
    private let storageDependencies: DeviceSyncServiceStorageDependencies
    private let restartDelayPolicy: DeviceSyncRestartDelayPolicy
    private let logger: LoggerProtocol
    private let exchangeDependencies: DeviceSyncExchangeDependencies
    private let peerEngineFactory: PeerEngineFactory

    private var peerEngines = [Data: any DeviceSyncPeerEngining]()
    private var deviceSubscriptionTask: Task<Void, Never>?

    private var configuration: DeviceSyncServiceConfiguration?

    init(
        ownStatementAccountId: Data,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        configFactory: WebRTCConfigMaking,
        peerConnectionFactory: RTCPeerConnectionFactory = WebRTCPeerConnectionFactoryProvider.make(),
        remoteContactResolver: RemoteContactResolving = RemoteContactOperationFactory(),
        storageDependencies: DeviceSyncServiceStorageDependencies = .init(),
        contactsStorageService: ContactsLocalStorageServicing = ContactsLocalStorageService(),
        storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared,
        updateIdProvider: DeviceSyncUpdateIdProviding = DeviceSyncUpdateIdProvider(),
        restartDelayPolicy: DeviceSyncRestartDelayPolicy = DeviceSyncRestartDelayPolicy(),
        logger: LoggerProtocol = Logger.shared,
        peerEngineFactory: @escaping PeerEngineFactory = { peerAccountId, contextFactory in
            DeviceSyncPeerEngine(
                peerStatementAccountId: peerAccountId,
                contextFactory: contextFactory
            )
        }
    ) {
        self.ownStatementAccountId = ownStatementAccountId
        self.configFactory = configFactory
        self.peerConnectionFactory = peerConnectionFactory
        self.storageDependencies = storageDependencies
        self.restartDelayPolicy = restartDelayPolicy
        self.logger = logger
        exchangeDependencies = DeviceSyncExchangeDependencies(
            remoteContactResolver: remoteContactResolver,
            deviceRepositoryFactory: storageDependencies.deviceRepositoryFactory,
            contactRepositoryFactory: storageDependencies.contactRepositoryFactory,
            chatRepositoryFactory: storageDependencies.chatRepositoryFactory,
            messageRepositoryFactory: storageDependencies.messageRepositoryFactory,
            removedChatRepositoryFactory: storageDependencies.removedChatRepositoryFactory,
            outgoingUpdateTimeRepositoryFactory: storageDependencies.outgoingUpdateTimeRepositoryFactory,
            contactDataProviderFactory: ChatContactDataProviderFactory(
                repositoryFactory: storageDependencies.contactRepositoryFactory,
                logger: logger
            ),
            messageDataProviderFactory: ChatMessageDataProviderFactory(
                repositoryFactory: storageDependencies.messageRepositoryFactory,
                logger: logger
            ),
            messageExchangeModeProvider: messageExchangeModeProvider,
            contactsStorageService: contactsStorageService,
            storageFacade: storageFacade,
            updateIdProvider: updateIdProvider
        )
        self.peerEngineFactory = peerEngineFactory
    }

    deinit {
        logger.debug("Deinit")
    }
}

// MARK: - DeviceSyncServicing

extension DeviceSyncService: DeviceSyncServicing {
    func setup(configuration: DeviceSyncServiceConfiguration) async {
        if self.configuration != nil {
            await throttle()
        }

        self.configuration = configuration

        startDeviceSubscription()
    }

    func throttle() async {
        let subscriptionTask = deviceSubscriptionTask
        subscriptionTask?.cancel()
        deviceSubscriptionTask = nil
        configuration = nil

        await subscriptionTask?.value

        let engines = Array(peerEngines.values)
        peerEngines.removeAll()
        await withTaskGroup(of: Void.self) { group in
            for engine in engines {
                group.addTask {
                    await engine.dispose()
                }
            }
        }
    }
}

// MARK: - Device Observation

private extension DeviceSyncService {
    func startDeviceSubscription() {
        deviceSubscriptionTask?.cancel()
        deviceSubscriptionTask = Task { [weak self] in
            await self?.runDeviceSubscription()
        }
    }

    func runDeviceSubscription() async {
        let stream = storageDependencies.deviceDataProviderFactory.subscribeDevices()

        for await devices in stream {
            guard !Task.isCancelled else { return }
            await handleDevicesUpdate(devices)
        }
    }

    func handleDevicesUpdate(_ devices: [Chat.LocalDevice]) async {
        let remoteDevices = devices.filter {
            $0.statementAccountId != ownStatementAccountId && $0.supportsDeviceSyncSession
        }
        let remoteAccountIds = Set(remoteDevices.map(\.statementAccountId))

        // Remove engines for devices that are no longer present
        let removedIds = Set(peerEngines.keys).subtracting(remoteAccountIds)
        var removedEngines = [any DeviceSyncPeerEngining]()
        for removedId in removedIds {
            logger.debug("Removing sync peer engine for device \(removedId.toHex())")
            if let engine = peerEngines.removeValue(forKey: removedId) {
                removedEngines.append(engine)
            }
        }

        await withTaskGroup(of: Void.self) { group in
            for engine in removedEngines {
                group.addTask {
                    await engine.dispose()
                }
            }

            // New peers start while unrelated removed peers finish shutting down.
            if !Task.isCancelled {
                for device in remoteDevices {
                    guard !Task.isCancelled else { break }
                    guard peerEngines[device.statementAccountId] == nil else {
                        continue
                    }
                    await startPeerEngine(for: device)
                }
            }
        }
    }

    func startPeerEngine(for device: Chat.LocalDevice) async {
        guard !Task.isCancelled else { return }

        let peerAccountId = device.statementAccountId
        let peerLogger = TaggedLogger(
            tag: "DeviceSync:\(peerAccountId.toHex().prefix(8))",
            logger: logger
        )
        let componentFactory = makeComponentFactory(for: device, peerLogger: peerLogger)
        let sessionDelegate = DeviceSyncPeerSessionDelegate(peerLogger: peerLogger)
        let contextFactory = DeviceSyncPeerEngineContextFactory(
            componentFactory: componentFactory,
            sessionDelegate: sessionDelegate,
            persistedOfferId: device.lastSyncOfferId,
            offerIdPersistenceHandler: makeOfferIdPersistenceHandler(for: peerAccountId),
            restartDelayPolicy: restartDelayPolicy,
            peerLogger: peerLogger
        )
        let engine = peerEngineFactory(peerAccountId, contextFactory)

        guard !Task.isCancelled else {
            await engine.dispose()
            return
        }

        peerEngines[peerAccountId] = engine
        peerLogger.debug("Starting peer engine")
        await engine.start()
    }

    func makeComponentFactory(
        for device: Chat.LocalDevice,
        peerLogger: LoggerProtocol
    ) -> DeviceSyncPeerComponentFactory {
        let role = determineSyncRole(peerAccountId: device.statementAccountId)
        let configFactory = configFactory
        let peerConnectionFactory = peerConnectionFactory
        let sessionFactory = DeviceSyncSessionFactory(peerLogger: peerLogger)

        return DeviceSyncPeerComponentFactory(
            sessionFactory: { [weak self] delegate, offerIdHandler in
                guard let self, let configuration = await configuration else {
                    throw DeviceSyncPeerEngineError.dependenciesUnavailable
                }
                return try await sessionFactory.makeSession(
                    device: device,
                    configuration: configuration,
                    role: role,
                    delegate: delegate,
                    offerIdHandler: offerIdHandler
                )
            },
            flowFactory: { session in
                DeviceSyncPeerConnectionFlow(
                    signaler: session,
                    role: role,
                    configFactory: configFactory,
                    peerConnectionFactory: peerConnectionFactory,
                    peerLogger: peerLogger
                )
            },
            exchangeFactory: { [weak self] flow, failureHandler in
                guard let self else {
                    throw DeviceSyncPeerEngineError.dependenciesUnavailable
                }
                guard let currentDevice = try await fetchDevice(device.statementAccountId) else {
                    throw DeviceSyncPeerEngineError.dependenciesUnavailable
                }
                return await makeExchange(
                    for: currentDevice,
                    flow: flow,
                    peerLogger: peerLogger,
                    failureHandler: failureHandler
                )
            }
        )
    }

    func fetchDevice(_ peerAccountId: Data) async throws -> Chat.LocalDevice? {
        try await storageDependencies.deviceRepositoryFactory
            .createRepository(forFilter: nil)
            .fetchOperation(by: { peerAccountId.toHex() }, options: .init())
            .asyncExecute()
    }

    func makeExchange(
        for device: Chat.LocalDevice,
        flow: any DeviceSyncPeerConnectionFlowing,
        peerLogger: LoggerProtocol,
        failureHandler: @escaping @Sendable (DeviceSyncConnectionFailure) async -> Void
    ) -> DeviceSyncExchange {
        DeviceSyncExchange(
            peer: .init(
                statementAccountId: device.statementAccountId,
                initialCheckpoint: device.outgoingUpdateTime
            ),
            flow: flow,
            dependencies: exchangeDependencies,
            peerLogger: peerLogger,
            behavior: .init(),
            handlers: .init(failure: failureHandler)
        )
    }

    func persistLastSyncOfferId(_ offerId: String, for peerAccountId: Data) async throws {
        let update = Chat.LastSyncOfferIdUpdate(
            statementAccountId: peerAccountId,
            lastSyncOfferId: offerId
        )

        let repository = storageDependencies.lastSyncOfferIdRepositoryFactory.createRepository(forFilter: nil)
        try await repository.saveOperation({ [update] }, { [] }).asyncExecute()
    }

    func determineSyncRole(peerAccountId: Data) -> CallRole {
        // Deterministic role: device with lexicographically smaller account ID initiates
        ownStatementAccountId.lexicographicallyPrecedes(peerAccountId)
            ? .initiator
            : .acceptor
    }
}

extension DeviceSyncService {
    /// Test seam for verifying durable offer-id persistence without constructing WebRTC resources.
    func makeOfferIdPersistenceHandler(
        for peerAccountId: Data
    ) -> @Sendable (String) async throws -> Void {
        { [weak self] offerId in
            guard let self else {
                throw DeviceSyncPeerEngineError.dependenciesUnavailable
            }
            try await persistLastSyncOfferId(offerId, for: peerAccountId)
        }
    }
}
