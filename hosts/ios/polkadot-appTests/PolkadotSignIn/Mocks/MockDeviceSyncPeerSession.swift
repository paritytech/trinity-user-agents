import Foundation
import MessageExchangeKit

struct MockDeviceSyncPeerSession: PeerSessionProtocol {
    typealias Message = Data

    let peer = MessageExchange.Peer(
        accountId: Data(repeating: 1, count: 32),
        publicKey: Data(repeating: 2, count: 33),
        pin: nil,
        devices: []
    )

    var sessionId: MessageExchange.SessionId {
        fatalError("sessionId is not used by device sync tests")
    }

    func addMessagesToQueue(_: [Data]) {}
}
