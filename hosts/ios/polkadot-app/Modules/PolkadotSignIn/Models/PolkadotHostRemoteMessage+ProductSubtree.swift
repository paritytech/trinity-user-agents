import Foundation
import MessageExchangeKit
import Products
import SubstrateSdk

extension PolkadotHostRemoteMessage {
    /// Consent-free fetch of a product's subtree public key (`ApProductSubtreeRequest`).
    struct ProductSubtreeRequest {
        let productId: ProductId
    }

    typealias ProductSubtreeResult = HostResult<ProductSubtreeKey, String>

    struct ProductSubtreeKey {
        static let length = 32

        /// sr25519 public key of `//product//{productId}`.
        let productPublicKey: Data
    }

    enum ProductSubtreeWireCodingError: Error {
        case invalidPublicKeyLength
    }
}

// MARK: - SCALE Coding

extension PolkadotHostRemoteMessage.ProductSubtreeRequest: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        productId = try String(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try productId.encode(scaleEncoder: scaleEncoder)
    }
}

extension PolkadotHostRemoteMessage.ProductSubtreeKey: MessageExchange.CodableMessage {
    init(scaleDecoder: any ScaleDecoding) throws {
        // `Sr25519PublicKey` is `[u8; 32]` on the wire — raw bytes, no length prefix.
        productPublicKey = try scaleDecoder.readAndConfirm(count: Self.length)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        guard productPublicKey.count == Self.length else {
            throw PolkadotHostRemoteMessage.ProductSubtreeWireCodingError.invalidPublicKeyLength
        }

        scaleEncoder.appendRaw(data: productPublicKey)
    }
}
