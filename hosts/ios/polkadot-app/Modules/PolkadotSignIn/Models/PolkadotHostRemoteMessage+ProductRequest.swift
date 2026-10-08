import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    /// The product a pairing host names as the origin of a request.
    struct ProductCaller {
        enum ExecutionKind: UInt8 {
            case app = 0
            case widget = 1
            case worker = 2
        }

        let productId: ProductId
        let executionKind: ExecutionKind
    }

    /// A product-originated request: its caller, followed by the payload fields.
    struct ProductRequest<Payload: MessageExchange.CodableMessage> {
        let caller: ProductCaller
        let payload: Payload
    }
}

extension PolkadotHostRemoteMessage.ProductCaller: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        productId = try String(scaleDecoder: scaleDecoder)
        guard let executionKind = try ExecutionKind(rawValue: UInt8(scaleDecoder: scaleDecoder)) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self.executionKind = executionKind
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try productId.encode(scaleEncoder: scaleEncoder)
        try executionKind.rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension PolkadotHostRemoteMessage.ProductRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        caller = try PolkadotHostRemoteMessage.ProductCaller(scaleDecoder: scaleDecoder)
        payload = try Payload(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try caller.encode(scaleEncoder: scaleEncoder)
        try payload.encode(scaleEncoder: scaleEncoder)
    }
}
