import Foundation
import KeyDerivation
import SubstrateSdk

/// Encoded as the 2-element JSON tuple `[productId, derivationIndex]` to match the wire format used by product scripts.
public struct ProductAccountId: Hashable, Codable {
    public let productId: ProductId
    public let derivationIndex: ProductAccountSelector

    public init(productId: ProductId, derivationIndex: ProductAccountSelector) {
        self.productId = productId
        self.derivationIndex = derivationIndex
    }

    /// Derivation path `//product//{productId}/{index}` with the index as a hex segment.
    public func derivationPath() throws -> String {
        try ProductDerivationPath.productAccount(
            productId: productId,
            index: derivationIndex.index32()
        )
    }

    public init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        productId = try container.decode(ProductId.self)
        derivationIndex = try container.decode(ProductAccountSelector.self)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(productId)
        try container.encode(derivationIndex)
    }
}

extension ProductAccountId: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        productId = try String(scaleDecoder: scaleDecoder)
        derivationIndex = try ProductAccountSelector(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try productId.encode(scaleEncoder: scaleEncoder)
        try derivationIndex.encode(scaleEncoder: scaleEncoder)
    }
}
