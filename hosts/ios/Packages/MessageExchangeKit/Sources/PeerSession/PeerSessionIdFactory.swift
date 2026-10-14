import Foundation

public struct PeerSessionInit {
    public let ownAccountId: MessageExchange.AccountId
    public let ownPin: String?
    public let peerAccountId: MessageExchange.AccountId
    public let peerPin: String?
    public let sharedSecret: Data

    public init(
        ownAccountId: MessageExchange.AccountId,
        ownPin: String?,
        peerAccountId: MessageExchange.AccountId,
        peerPin: String?,
        sharedSecret: Data
    ) {
        self.ownAccountId = ownAccountId
        self.ownPin = ownPin
        self.peerAccountId = peerAccountId
        self.peerPin = peerPin
        self.sharedSecret = sharedSecret
    }
}

public protocol PeerSessionIdFactoryProtocol {
    func createSessionId(for params: PeerSessionInit) throws -> MessageExchange.SessionId
}

public final class PeerSessionIdFactory {
    public init() {}
}

extension PeerSessionIdFactory: PeerSessionIdFactoryProtocol {
    public func createSessionId(for params: PeerSessionInit) throws -> MessageExchange.SessionId {
        let own = try makeRawSessionId(
            firstAccountId: params.ownAccountId,
            secondAccountId: params.peerAccountId,
            firstPin: params.ownPin,
            secondPin: params.peerPin,
            sharedSecred: params.sharedSecret
        )
        let peer = try makeRawSessionId(
            firstAccountId: params.peerAccountId,
            secondAccountId: params.ownAccountId,
            firstPin: params.peerPin,
            secondPin: params.ownPin,
            sharedSecred: params.sharedSecret
        )
        return .init(
            own: own.id,
            peer: peer.id,
            ownParameter: own.parameter,
            peerParameter: peer.parameter
        )
    }
}

private extension PeerSessionIdFactory {
    var pinSeparator: Data {
        Data("/".utf8)
    }

    func makeRawSessionId(
        firstAccountId: MessageExchange.AccountId,
        secondAccountId: MessageExchange.AccountId,
        firstPin: String?,
        secondPin: String?,
        sharedSecred: Data
    ) throws -> (parameter: Data, id: Data) {
        let parameter = firstAccountId
            + secondAccountId
            + makePinPart(value: firstPin)
            + makePinPart(value: secondPin)
        let dataToHash = Data("session".utf8) + parameter
        let id = try dataToHash.blake2b32WithKey(sharedSecred)
        return (parameter, id)
    }

    func makePinPart(value: String?) -> Data {
        guard let value else {
            return pinSeparator
        }
        return pinSeparator + Data(value.utf8)
    }
}
