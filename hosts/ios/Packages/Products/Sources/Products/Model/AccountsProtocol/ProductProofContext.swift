import Foundation
import SubstrateSdk

/// Product-scoped proof context. The product prefix is a privacy boundary:
/// it prevents one product from choosing a suffix that collides with another product's context.
/// The suffix is an account selector expanding to the same 32-byte derivation
/// index as the account it aliases.
public struct ProductProofContext: Hashable, Codable {
    public let productId: ProductId
    public let suffix: ProductAccountSelector

    public init(productId: ProductId, suffix: ProductAccountSelector) {
        self.productId = productId
        self.suffix = suffix
    }

    /// RFC-0004: `blake2b256(utf8("product/") ++ utf8(productId) ++ utf8("/") ++ index32(suffix))`.
    public func contextBytes() throws -> Data {
        let prefix = Data("product/\(productId)/".utf8)
        return try (prefix + suffix.index32().bytes).blake2b32()
    }
}
