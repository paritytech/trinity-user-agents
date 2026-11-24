import SubstrateSdk
@testable import MessageExchangeKit

struct TestMessage: MessageExchange.CodableMessage {
    let id: UInt32
    let route: PeerSessionRoute

    init(
        id: UInt32,
        route: PeerSessionRoute = .device
    ) {
        self.id = id
        self.route = route
    }

    init(scaleDecoder: any ScaleDecoding) throws {
        id = try UInt32(scaleDecoder: scaleDecoder)
        route = .device
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try id.encode(scaleEncoder: scaleEncoder)
    }
}
