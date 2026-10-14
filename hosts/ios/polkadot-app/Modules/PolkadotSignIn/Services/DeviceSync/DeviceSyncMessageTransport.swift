import Foundation
import Foundation_iOS
import CryptoKit
import SubstrateSdk
import StatementStore
import MessageExchangeKit
import AsyncExtensions
import StructuredConcurrency

// MARK: - Protocol

protocol DeviceSyncMessageTransporting: Sendable {
    func open(delegate: AnyPeerSessionDelegate<Data>) async throws
    func close() async
    func send(_ messages: [Data]) async
}

// MARK: - Implementation

/// Encrypted message transport between two devices over the statement store,
/// backed by the `MessageExchangeKit` infrastructure (`PeerSession`).
///
/// Used by `DeviceSyncSignalingSession` to carry WebRTC signaling
/// messages for the device sync data channel.
final class DeviceSyncMessageTransport: @unchecked Sendable {
    private let serviceFactory: MessageExchangeServiceFactory
    private let connection: StatementStoreConnecting
    private let peer: MessageExchange.Peer
    private let ownSignKeyId: String
    private let ownEncryptionKeyId: String
    private let peerLogger: LoggerProtocol

    private let state = DeviceSyncMessageTransportState()

    init(
        connection: StatementStoreConnecting,
        signerManager: StatementStoreSignerManaging,
        encryptionManager: MessageExchangeEncryptionManaging,
        ownSignKeyId: String,
        ownEncryptionKeyId: String,
        peerStatementAccountId: Data,
        peerEncryptionPublicKey: Data,
        peerLogger: LoggerProtocol
    ) {
        self.connection = connection
        self.ownSignKeyId = ownSignKeyId
        self.ownEncryptionKeyId = ownEncryptionKeyId
        self.peerLogger = peerLogger

        peer = MessageExchange.Peer(
            accountId: peerStatementAccountId,
            publicKey: peerEncryptionPublicKey,
            pin: nil,
            devices: []
        )

        serviceFactory = MessageExchangeServiceFactory(
            messageExchangeModeProvider: FixedMessageExchangeModeProvider(mode: .identity),
            signManager: signerManager,
            encryptionManager: encryptionManager,
            deviceEncryptionKeyFactory: nil,
            messageRouteSelector: { _ in .identity },
            maxStatementSize: MessageExchangeCoordinatorFactory.Constants.maxDeviceSyncStatementSize,
            workQueue: DispatchQueue(label: "device.sync.transport.queue"),
            logger: peerLogger
        )
    }

    deinit {
        peerLogger.debug("Deinit")
    }
}

private actor DeviceSyncMessageTransportState {
    private var exchangeService: AnyMessageExchangeService<Data>?
    private var isClosed = false

    func installServiceIfOpen(_ service: AnyMessageExchangeService<Data>) -> Bool {
        guard !isClosed else { return false }
        exchangeService = service
        return true
    }

    func currentServiceIfOpen() -> AnyMessageExchangeService<Data>? {
        guard !isClosed else { return nil }
        return exchangeService
    }

    func close() {
        isClosed = true
        exchangeService = nil
    }
}

// MARK: - DeviceSyncMessageTransporting

extension DeviceSyncMessageTransport: DeviceSyncMessageTransporting {
    func open(delegate: AnyPeerSessionDelegate<Data>) async throws {
        let service = try serviceFactory.makeService(
            statementStoreConnection: connection,
            delegate: delegate
        )

        let own = MessageExchange.Own(
            signKeyId: ownSignKeyId,
            encryptionKeyId: ownEncryptionKeyId,
            pin: nil
        )

        let request = MessageExchange.SessionRequest(own: own, peer: peer)
        guard await state.installServiceIfOpen(service) else {
            throw CancellationError()
        }
        service.updateSessions([request])
    }

    func close() async {
        await state.close()
    }

    func send(_ messages: [Data]) async {
        guard !messages.isEmpty else {
            return
        }

        guard let service = await state.currentServiceIfOpen() else {
            peerLogger.error("Cannot send - not open")
            return
        }

        service.addMessagesToQueue(messages, for: peer)
    }
}
