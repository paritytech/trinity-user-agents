import Foundation
import MessageExchangeKit

extension Chat.RemoteContact {
    func toMessageExchangePeer() -> MessageExchange.Peer {
        MessageExchange.Peer(
            accountId: accountId,
            publicKey: chatPublicKey.rawData,
            pin: nil,
            devices: []
        )
    }
}
