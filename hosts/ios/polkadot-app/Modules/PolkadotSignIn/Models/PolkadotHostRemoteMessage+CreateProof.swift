import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    /// RFC-0004 Accounts Protocol `create_account_proof` request.
    struct CreateProofRequest {
        let callingProductId: ProductId
        let context: ProductProofContext
        let ring: RingLocation
        let message: Data
    }

    typealias CreateProofResult = HostResult<RingVrfProof, RingVrfError>
}

extension PolkadotHostRemoteMessage.CreateProofRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        callingProductId = try String(scaleDecoder: scaleDecoder)
        context = try ProductProofContext(scaleDecoder: scaleDecoder)
        ring = try RingLocation(scaleDecoder: scaleDecoder)
        message = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try callingProductId.encode(scaleEncoder: scaleEncoder)
        try context.encode(scaleEncoder: scaleEncoder)
        try ring.encode(scaleEncoder: scaleEncoder)
        try message.encode(scaleEncoder: scaleEncoder)
    }
}
