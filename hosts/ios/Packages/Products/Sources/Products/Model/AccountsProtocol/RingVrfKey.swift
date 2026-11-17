import struct TrUAPIHost.ProductId
import Foundation
import SubstrateSdk

public struct RingVrfKeyHandle: Hashable {
    public let productId: ProductId
    public let index: ProductAccountSelector

    public init(productId: ProductId, index: ProductAccountSelector) {
        self.productId = productId
        self.index = index
    }
}

extension RingVrfKeyHandle: Codable {
    public init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        productId = try container.decode(ProductId.self)
        index = try container.decode(ProductAccountSelector.self)
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.unkeyedContainer()
        try container.encode(productId)
        try container.encode(index)
    }
}

extension RingVrfKeyHandle: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        productId = try ProductId(String(scaleDecoder: scaleDecoder))
        index = try ProductAccountSelector(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try productId.rawValue.encode(scaleEncoder: scaleEncoder)
        try index.encode(scaleEncoder: scaleEncoder)
    }
}

public enum RingVrfKeyDisclosure: String, Codable, Hashable, Sendable {
    case anonymized = "Anonymized"
    case publicKey = "PublicKey"
}

public struct RegisteredRingVrfKey: Hashable, Codable {
    public let handle: RingVrfKeyHandle
    public let rings: [RingLocation]
    @OptionHexCodable public var publicKey: Data?

    public init(handle: RingVrfKeyHandle, rings: [RingLocation], publicKey: Data?) {
        self.handle = handle
        self.rings = rings
        _publicKey = OptionHexCodable(wrappedValue: publicKey)
    }
}
