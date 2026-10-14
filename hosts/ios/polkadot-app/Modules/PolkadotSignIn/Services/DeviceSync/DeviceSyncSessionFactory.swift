import Foundation
import Foundation_iOS
import MessageExchangeKit

final class DeviceSyncSessionFactory {
    private let peerLogger: LoggerProtocol

    init(peerLogger: LoggerProtocol) {
        self.peerLogger = peerLogger
    }

    func makeSession(
        device: Chat.LocalDevice,
        configuration: DeviceSyncServiceConfiguration,
        role: CallRole,
        delegate: AnyPeerSessionDelegate<Data>,
        offerIdHandler: @escaping @Sendable (String) async throws -> Void
    ) async throws -> DeviceSyncSignalingSession {
        let transport = DeviceSyncMessageTransport(
            connection: configuration.connection,
            signerManager: configuration.signerManager,
            encryptionManager: configuration.encryptionManager,
            ownSignKeyId: configuration.ownSignKeyId,
            ownEncryptionKeyId: configuration.ownEncryptionKeyId,
            peerStatementAccountId: device.statementAccountId,
            peerEncryptionPublicKey: device.encryptionPublicKey,
            peerLogger: peerLogger
        )
        try await transport.open(delegate: delegate)
        return DeviceSyncSignalingSession(
            transport: transport,
            role: role,
            offerIdHandler: offerIdHandler,
            peerLogger: peerLogger
        )
    }
}
