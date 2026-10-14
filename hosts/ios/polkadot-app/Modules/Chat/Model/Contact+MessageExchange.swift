import Foundation
import MessageExchangeKit

extension Chat.Contact {
    func toMessageExchangeSessionRequest() -> MessageExchange.SessionRequest {
        MessageExchange.SessionRequest(
            own: ownKeyId.toMessageExchangeOwn(),
            peer: toMessageExchangePeer()
        )
    }

    func toMessageExchangePeer() -> MessageExchange.Peer {
        let peerDevices = devices.map {
            MessageExchange.DeviceInfo(
                statementAccountId: $0.statementAccountId,
                encryptionPublicKey: $0.encryptionPublicKey
            )
        }

        return MessageExchange.Peer(
            accountId: accountId,
            publicKey: publicKey,
            pin: pin,
            devices: peerDevices
        )
    }
}

extension Chat.Contact.Own {
    func toMessageExchangeOwn() -> MessageExchange.Own {
        MessageExchange.Own(
            signKeyId: signKeyId,
            encryptionKeyId: encryptionKeyId,
            pin: nil
        )
    }
}
