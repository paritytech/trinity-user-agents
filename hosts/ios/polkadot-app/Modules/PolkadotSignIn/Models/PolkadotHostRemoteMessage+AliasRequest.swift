import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    /// RFC-0004 Accounts Protocol `get_account_alias` request. The calling product
    /// is named explicitly because the host acts on a product's behalf here.
    struct AliasRequest {
        let callingProductId: ProductId
        let context: ProductProofContext
        let ring: RingLocation
    }
}

extension PolkadotHostRemoteMessage.AliasRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        callingProductId = try String(scaleDecoder: scaleDecoder)
        context = try ProductProofContext(scaleDecoder: scaleDecoder)
        ring = try RingLocation(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try callingProductId.encode(scaleEncoder: scaleEncoder)
        try context.encode(scaleEncoder: scaleEncoder)
        try ring.encode(scaleEncoder: scaleEncoder)
    }
}
