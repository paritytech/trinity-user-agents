import Foundation
import struct TrUAPIHost.ProductId

/// Derivation paths scoped to a validated product identity.
public enum ProductDerivationPath {
    public static func productRoot(productId: ProductId) -> String {
        "//product//\(productId)"
    }

    public static func productAccount(productId: ProductId, index: DerivationIndex32) -> String {
        "\(productRoot(productId: productId))/\(index.asPathSegment())"
    }
}
