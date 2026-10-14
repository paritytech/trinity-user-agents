import Foundation
import CryptoKit
import StatementStore
import SDKLogger
import SubstrateSdk

enum PeerSessionFactoryError: Error {
    case deviceEncryptionKeyFactoryMissing
}

public final class PeerSessionFactory<M: MessageExchange.CodableMessage> {
    public typealias Message = M

    private weak var delegate: AnyPeerSessionDelegate<M>?
    private let workQueue: DispatchQueue
    private let submitter: StatementStoreSubmitting
    private let signerManager: StatementStoreSignerManaging
    private let encryptionManager: MessageExchangeEncryptionManaging
    private let deviceEncryptionKeyFactory: MessageExchangeEncryptionMaking?
    private let messageExchangeModeProvider: MessageExchangeModeProviding
    private let sessionIdFactory: PeerSessionIdFactoryProtocol
    private let channelFactory: StatementChannelMaking
    private let preSendHandler: AnyPeerSessionPreSendHandler<M>
    private let preferredRouteSelector: PeerSessionRouteSelector<M>
    private let subscriptionFactory: StatementSubscriptionFactoryProtocol
    private let maxStatementSize: Int
    private let compactorFactory: AnyMessageCompactorFactory<M>?
    private let operationQueue: OperationQueue
    private let logger: SDKLoggerProtocol?

    public init(
        delegate: AnyPeerSessionDelegate<M>,
        workQueue: DispatchQueue,
        submitter: StatementStoreSubmitting,
        encryptionManager: MessageExchangeEncryptionManaging,
        deviceEncryptionKeyFactory: MessageExchangeEncryptionMaking?,
        messageExchangeModeProvider: MessageExchangeModeProviding,
        signerManager: StatementStoreSignerManaging,
        sessionIdFactory: PeerSessionIdFactoryProtocol,
        channelFactory: StatementChannelMaking,
        preSendHandler: AnyPeerSessionPreSendHandler<M>,
        preferredRouteSelector: PeerSessionRouteSelector<M>,
        subscriptionFactory: StatementSubscriptionFactoryProtocol,
        maxStatementSize: Int,
        compactorFactory: AnyMessageCompactorFactory<M>? = nil,
        operationQueue: OperationQueue,
        logger: SDKLoggerProtocol?
    ) {
        self.delegate = delegate
        self.workQueue = workQueue
        self.submitter = submitter
        self.encryptionManager = encryptionManager
        self.deviceEncryptionKeyFactory = deviceEncryptionKeyFactory
        self.messageExchangeModeProvider = messageExchangeModeProvider
        self.signerManager = signerManager
        self.sessionIdFactory = sessionIdFactory
        self.channelFactory = channelFactory
        self.preSendHandler = preSendHandler
        self.preferredRouteSelector = preferredRouteSelector
        self.subscriptionFactory = subscriptionFactory
        self.maxStatementSize = maxStatementSize
        self.compactorFactory = compactorFactory
        self.operationQueue = operationQueue
        self.logger = logger
    }
}

extension PeerSessionFactory: PeerSessionMaking {
    public func makeSession(
        for request: MessageExchange.SessionRequest
    ) -> AnyPeerSession<M>? {
        do {
            let signer = try signerManager.makeSigner(for: request.own.signKeyId)

            let identityEncryptionKeyFactory = try encryptionManager
                .makeEncryptorFactory(ownEncryptionKeyId: request.own.encryptionKeyId)

            let identityEncryptor = try identityEncryptionKeyFactory
                .makeEncryptor(remotePublicKey: request.peer.publicKey)

            let messageExchangeMode = messageExchangeModeProvider.mode(for: request.own)

            let routeConfiguration = makeRouteConfiguration(
                messageExchangeMode: messageExchangeMode,
                hasPeerDevices: request.hasPeerDevices
            )

            let outgoingSharedSecret = try deriveOutgoingSharedSecret(
                for: request,
                messageExchangeMode: messageExchangeMode,
                identitySharedSecret: identityEncryptor.sharedSecret
            )

            let primarySessionId = try sessionIdFactory.createSessionId(
                for: .init(
                    ownAccountId: signer.accountId,
                    ownPin: request.own.pin,
                    peerAccountId: request.peer.accountId,
                    peerPin: request.peer.pin,
                    sharedSecret: outgoingSharedSecret
                )
            )
            let identitySessionId = try sessionIdFactory.createSessionId(
                for: .init(
                    ownAccountId: signer.accountId,
                    ownPin: request.own.pin,
                    peerAccountId: request.peer.accountId,
                    peerPin: request.peer.pin,
                    sharedSecret: identityEncryptor.sharedSecret
                )
            )

            let priorityProvider = PeerSessionPriorityProvider(
                logger: logger
            )

            let statementDataCoder = try makeStatementDataCoder(
                for: request,
                messageExchangeMode: messageExchangeMode,
                identityEncryptor: identityEncryptor,
                identityEncryptionKeyFactory: identityEncryptionKeyFactory,
                signer: signer
            )

            let routeContexts = try makeRouteContexts(
                identitySessionId: identitySessionId,
                primarySessionId: primarySessionId
            )

            let sizeValidator = OutgoingRequestSizeValidator(maxStatementSize: maxStatementSize)

            let outgoingChannel = try makeOutgoingChannel(
                routeContexts: routeContexts,
                signer: signer,
                priorityProvider: priorityProvider,
                requestQueue: AnyOutgoingRequestQueue(OutgoingRequestQueue(
                    statementDataCoder: statementDataCoder,
                    routeSelector: routeConfiguration.effectiveRouteSelector,
                    sizeValidator: sizeValidator,
                    supportedRoutes: routeConfiguration.supportedRoutes,
                    logger: logger
                ))
            )

            let messageCompacter = compactorFactory?.createCompactor(for: request.own.signKeyId)

            let outgoingInterface: AnyOutgoingMessageChannel<M>
            var compactionProxy: OutgoingChannelCompactionProxy<M>?

            if let messageCompacter {
                let proxy = OutgoingChannelCompactionProxy(
                    channel: AnyOutgoingMessageChannel(outgoingChannel),
                    compacter: messageCompacter,
                    routeSelector: routeConfiguration.effectiveRouteSelector,
                    sizeValidator: sizeValidator,
                    statementDataCoder: statementDataCoder,
                    workQueue: workQueue,
                    logger: logger
                )

                let outgoingChannelDelegate = AnyOutgoingMessageChannelDelegate(proxy)

                outgoingChannel.delegate = outgoingChannelDelegate
                compactionProxy = proxy
                outgoingInterface = AnyOutgoingMessageChannel(proxy)
            } else {
                outgoingInterface = AnyOutgoingMessageChannel(outgoingChannel)
            }

            let incomingChannel = try makeIncomingChannel(
                routeContexts: routeContexts,
                signer: signer,
                priorityProvider: priorityProvider,
                statementDataCoder: statementDataCoder
            )

            let ownSubscription = try makeOwnSubscription(
                for: request,
                messageExchangeMode: messageExchangeMode,
                identitySessionId: identitySessionId,
                primarySessionId: primarySessionId,
                signer: signer
            )

            let peerSubscription = try makePeerSubscription(
                for: request,
                messageExchangeMode: messageExchangeMode,
                identitySessionId: identitySessionId,
                identityEncryptionKeyFactory: identityEncryptionKeyFactory,
                signer: signer
            )

            let sessionInitializer = PeerSessionInitializer<Message>(
                priorityProvider: priorityProvider,
                ownSubscription: ownSubscription,
                peerSubscription: peerSubscription,
                workQueue: workQueue,
                statementDataCoder: statementDataCoder,
                supportedRoutes: routeConfiguration.supportedRoutes,
                logger: logger
            )

            let session = PeerSession<Message>(
                workQueue: workQueue,
                peer: request.peer,
                sessionId: primarySessionId,
                outgoingChannel: outgoingInterface,
                incomingChannel: AnyIncomingMessageChannel(incomingChannel),
                peerSubscription: peerSubscription,
                initializer: sessionInitializer,
                priorityProvider: priorityProvider,
                statementDataCoder: statementDataCoder,
                routeContexts: routeContexts,
                logger: logger
            )

            let initializerDelegate = AnyPeerSessionInitializerDelegate(session)
            let incomingChannelDelegate = AnyIncomingMessageChannelDelegate(session)
            let outgoingChannelDelegate = AnyOutgoingMessageChannelDelegate(session)

            if let compactionProxy {
                compactionProxy.delegate = outgoingChannelDelegate
            } else {
                outgoingChannel.delegate = outgoingChannelDelegate
            }

            session.delegate = delegate
            incomingChannel.delegate = incomingChannelDelegate
            sessionInitializer.delegate = initializerDelegate

            return AnyPeerSession(session)
        } catch {
            logger?.error("Unexpected error: \(error)")
            return nil
        }
    }
}

private extension PeerSessionFactory {
    struct RouteConfiguration<Message: MessageExchange.CodableMessage> {
        let supportedRoutes: [PeerSessionRoute]
        let effectiveRouteSelector: PeerSessionRouteSelector<Message>
    }

    func makeRouteConfiguration(
        messageExchangeMode: MessageExchangeMode,
        hasPeerDevices: Bool
    ) -> RouteConfiguration<Message> {
        // When only the identity lane exists, every message must also use the
        // identity route. The preferred selector is only valid once all selected
        // routes have matching lanes.
        switch messageExchangeMode {
        case .identity:
            identityOnlyRouteConfiguration()
        case .multidevice:
            if hasPeerDevices {
                .init(
                    supportedRoutes: [.identity, .device],
                    effectiveRouteSelector: preferredRouteSelector
                )
            } else {
                identityOnlyRouteConfiguration()
            }
        }
    }

    func identityOnlyRouteConfiguration() -> RouteConfiguration<Message> {
        .init(
            supportedRoutes: [.identity],
            effectiveRouteSelector: .init { _ in .identity }
        )
    }

    func deriveOutgoingSharedSecret(
        for request: MessageExchange.SessionRequest,
        messageExchangeMode: MessageExchangeMode,
        identitySharedSecret: Data
    ) throws -> Data {
        switch messageExchangeMode {
        case .identity:
            return identitySharedSecret
        case .multidevice:
            guard request.hasPeerDevices else {
                return identitySharedSecret
            }
            guard let deviceEncryptionKeyFactory else {
                throw PeerSessionFactoryError.deviceEncryptionKeyFactoryMissing
            }
            let deviceEncryptor = try deviceEncryptionKeyFactory
                .makeEncryptor(remotePublicKey: request.peer.publicKey)
            return deviceEncryptor.sharedSecret
        }
    }

    func makeStatementDataCoder(
        for request: MessageExchange.SessionRequest,
        messageExchangeMode: MessageExchangeMode,
        identityEncryptor: MessageExchangeEncrypting,
        identityEncryptionKeyFactory: MessageExchangeEncryptionMaking,
        signer: StatementStoreSigning
    ) throws -> StatementDataCoding {
        let identityCoder = StatementDataCoder(encryptor: identityEncryptor, logger: logger)

        switch messageExchangeMode {
        case .identity:
            return identityCoder
        case .multidevice:
            guard request.hasPeerDevices else {
                // Peer devices are not yet known during the pre-handshake window
                // (before DeviceChatAccepted is exchanged). Fall back to identity
                // encryption so the session can be established and the handshake
                // can complete. Once devices are stored, updateSessions will
                // recreate the session with the full device-aware coder.
                // The same logic applies to `deriveOutgoingSharedSecret` and
                // `makePeerSubscription`.
                return identityCoder
            }
            guard let deviceEncryptionKeyFactory else {
                throw PeerSessionFactoryError.deviceEncryptionKeyFactoryMissing
            }
            let deviceCoder = try makeMultiDeviceAwareCoder(
                for: request,
                identityCoder: identityCoder,
                identityEncryptionKeyFactory: identityEncryptionKeyFactory,
                deviceEncryptionKeyFactory: deviceEncryptionKeyFactory,
                signer: signer
            )
            return deviceCoder
        }
    }

    func makeMultiDeviceAwareCoder(
        for request: MessageExchange.SessionRequest,
        identityCoder: StatementDataCoding,
        identityEncryptionKeyFactory: MessageExchangeEncryptionMaking,
        deviceEncryptionKeyFactory: MessageExchangeEncryptionMaking,
        signer: StatementStoreSigning
    ) throws -> StatementDataCoding {
        let outgoingCoder = try makeMultiDeviceOutgoingCoder(
            deviceEncryptionKeyFactory: deviceEncryptionKeyFactory,
            peerPublicKey: request.peer.publicKey
        )
        let incomingCoders = try makeMultiDeviceIncomingCoders(
            peerDevices: request.peer.devices,
            identityEncryptionKeyFactory: identityEncryptionKeyFactory
        )
        return MultiDeviceAwareStatementDataCoder(
            outgoingCoder: outgoingCoder,
            identityCoder: identityCoder,
            incomingCoders: incomingCoders,
            multiDeviceCoder: MultiDeviceStatementDataCoder(
                deviceEncryptionKeyFactory: deviceEncryptionKeyFactory,
                logger: logger
            ),
            recipientDevices: makeRecipientDevices(peerDevices: request.peer.devices),
            ownStatementAccountId: signer.accountId,
            deviceKeysByAccountId: makeDeviceKeysByAccountId(peerDevices: request.peer.devices),
            logger: logger
        )
    }

    func makeMultiDeviceOutgoingCoder(
        deviceEncryptionKeyFactory: MessageExchangeEncryptionMaking,
        peerPublicKey: Data
    ) throws -> StatementDataCoding {
        let encryptor = try deviceEncryptionKeyFactory.makeEncryptor(remotePublicKey: peerPublicKey)
        return StatementDataCoder(encryptor: encryptor, logger: logger)
    }

    func makeMultiDeviceIncomingCoders(
        peerDevices: [MessageExchange.DeviceInfo],
        identityEncryptionKeyFactory: MessageExchangeEncryptionMaking
    ) throws -> [AccountId: StatementDataCoding] {
        try Dictionary(
            peerDevices.map { device in
                let encryptor = try identityEncryptionKeyFactory.makeEncryptor(
                    remotePublicKey: device.encryptionPublicKey
                )
                return (
                    device.statementAccountId,
                    StatementDataCoder(encryptor: encryptor, logger: logger) as StatementDataCoding
                )
            },
            uniquingKeysWith: { _, latest in latest }
        )
    }

    func makeRecipientDevices(peerDevices: [MessageExchange.DeviceInfo]) -> [RecipientDeviceInfo] {
        peerDevices.map {
            RecipientDeviceInfo(
                statementAccountId: $0.statementAccountId,
                encryptionPublicKey: $0.encryptionPublicKey
            )
        }
    }

    func makeDeviceKeysByAccountId(peerDevices: [MessageExchange.DeviceInfo]) -> [AccountId: Data] {
        Dictionary(
            peerDevices.map { ($0.statementAccountId, $0.encryptionPublicKey) },
            uniquingKeysWith: { _, latest in latest }
        )
    }

    func makePeerSubscription(
        for request: MessageExchange.SessionRequest,
        messageExchangeMode: MessageExchangeMode,
        identitySessionId: MessageExchange.SessionId,
        identityEncryptionKeyFactory: MessageExchangeEncryptionMaking,
        signer: StatementStoreSigning
    ) throws -> PeerSessionStatementSubscribing {
        let identityStatementSubscription = try subscriptionFactory.createSubscription(for: .init(
            accountId: request.peer.accountId,
            rawSessionId: identitySessionId.peer
        ))

        switch messageExchangeMode {
        case .identity:
            return makeStatementSubscription(inputs: [
                .init(
                    route: .identity,
                    subscription: identityStatementSubscription
                )
            ])
        case .multidevice:
            guard request.hasPeerDevices else {
                return makeStatementSubscription(inputs: [
                    .init(
                        route: .identity,
                        subscription: identityStatementSubscription
                    )
                ])
            }

            var inputs: [PeerSessionStatementSubscription.Input] = [
                .init(
                    route: .identity,
                    subscription: identityStatementSubscription
                )
            ]

            let deviceInputs = try request.peer.devices.map { device in
                let deviceEncryptor = try identityEncryptionKeyFactory
                    .makeEncryptor(remotePublicKey: device.encryptionPublicKey)
                let deviceSessionId = try sessionIdFactory.createSessionId(
                    for: .init(
                        ownAccountId: signer.accountId,
                        ownPin: request.own.pin,
                        peerAccountId: device.statementAccountId,
                        peerPin: request.peer.pin,
                        sharedSecret: deviceEncryptor.sharedSecret
                    )
                )
                let deviceStatementSubscription = try subscriptionFactory.createSubscription(for: .init(
                    accountId: device.statementAccountId,
                    rawSessionId: deviceSessionId.peer
                ))
                return PeerSessionStatementSubscription.Input(
                    route: .device,
                    subscription: deviceStatementSubscription
                )
            }

            inputs.append(contentsOf: deviceInputs)

            return makeStatementSubscription(inputs: inputs)
        }
    }

    func makeOwnSubscription(
        for request: MessageExchange.SessionRequest,
        messageExchangeMode: MessageExchangeMode,
        identitySessionId: MessageExchange.SessionId,
        primarySessionId: MessageExchange.SessionId,
        signer: StatementStoreSigning
    ) throws -> PeerSessionStatementSubscribing {
        let identityStatementSubscription = try subscriptionFactory.createSubscription(for: .init(
            accountId: signer.accountId,
            rawSessionId: identitySessionId.own
        ))

        switch messageExchangeMode {
        case .identity:
            return makeStatementSubscription(inputs: [
                .init(
                    route: .identity,
                    subscription: identityStatementSubscription
                )
            ])
        case .multidevice:
            guard request.hasPeerDevices else {
                return makeStatementSubscription(inputs: [
                    .init(
                        route: .identity,
                        subscription: identityStatementSubscription
                    )
                ])
            }

            let deviceStatementSubscription = try subscriptionFactory.createSubscription(for: .init(
                accountId: signer.accountId,
                rawSessionId: primarySessionId.own
            ))

            return makeStatementSubscription(inputs: [
                .init(
                    route: .identity,
                    subscription: identityStatementSubscription
                ),
                .init(
                    route: .device,
                    subscription: deviceStatementSubscription
                )
            ])
        }
    }

    func makeStatementSubscription(
        inputs: [PeerSessionStatementSubscription.Input]
    ) -> PeerSessionStatementSubscribing {
        PeerSessionStatementSubscription(inputs: inputs, workQueue: workQueue)
    }

    func makeRouteContexts(
        identitySessionId: MessageExchange.SessionId,
        primarySessionId: MessageExchange.SessionId
    ) throws -> PeerSessionRoute.Contexts {
        try PeerSessionRoute.Contexts(
            device: makeRouteContext(for: primarySessionId),
            identity: makeRouteContext(for: identitySessionId)
        )
    }

    func makeRouteContext(
        for sessionId: MessageExchange.SessionId
    ) throws -> PeerSessionRoute.Context {
        try PeerSessionRoute.Context(
            sessionId: sessionId,
            requestChannelId: channelFactory.createRequestChannel(for: sessionId),
            responseChannelId: channelFactory.createResponseChannel(for: sessionId),
            peerRequestChannelId: channelFactory.createPeerRequestChannel(for: sessionId)
        )
    }

    func makeOutgoingChannel(
        routeContexts: PeerSessionRoute.Contexts,
        signer: StatementStoreSigning,
        priorityProvider: PeerSessionPriorityProviding,
        requestQueue: AnyOutgoingRequestQueue<Message>
    ) throws -> OutgoingMessageChannel<M> {
        OutgoingMessageChannel(
            workQueue: workQueue,
            routeContexts: routeContexts,
            submitter: submitter,
            signer: signer,
            preSendHandler: preSendHandler,
            priorityProvider: priorityProvider,
            requestQueue: requestQueue,
            operationQueue: operationQueue,
            logger: logger
        )
    }

    func makeIncomingChannel(
        routeContexts: PeerSessionRoute.Contexts,
        signer: StatementStoreSigning,
        priorityProvider: PeerSessionPriorityProviding,
        statementDataCoder: StatementDataCoding
    ) throws -> IncomingMessageChannel<M> {
        IncomingMessageChannel(
            workQueue: workQueue,
            routeContexts: routeContexts,
            submitter: submitter,
            signer: signer,
            priorityProvider: priorityProvider,
            statementDataCoder: statementDataCoder,
            operationQueue: operationQueue,
            logger: logger
        )
    }
}
