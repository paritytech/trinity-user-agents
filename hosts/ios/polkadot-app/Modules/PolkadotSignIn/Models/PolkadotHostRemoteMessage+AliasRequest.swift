import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    /// RFC-0004 Accounts Protocol `get_account_alias` request.
    struct AliasRequest {
        let context: ProductProofContext
        let ring: RingLocation
    }
}

extension PolkadotHostRemoteMessage.AliasRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        context = try ProductProofContext(scaleDecoder: scaleDecoder)
        ring = try RingLocation(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try context.encode(scaleEncoder: scaleEncoder)
        try ring.encode(scaleEncoder: scaleEncoder)
    }
}
