import Foundation

public enum ProductDerivationPathError: Error, Equatable {
    case invalidProductId(String)
}

/// Derivation paths for product accounts:
/// `//product//{productId}/{index}` — hard namespace and product junctions, soft 32-byte
/// index rendered as a hex segment (the junction parser maps hex straight to the raw
/// chain code).
public enum ProductDerivationPath {
    /// `//product//{productId}` — the product subtree root behind the hard-junction
    /// firewall. Validated so a dynamic product id cannot inject extra junctions.
    public static func productRoot(productId: String) throws -> String {
        guard !productId.isEmpty, !productId.contains("/") else {
            throw ProductDerivationPathError.invalidProductId(productId)
        }

        return root(productId)
    }

    /// `//product//{productId}/{index}` — an account within the product subtree.
    public static func productAccount(productId: String, index: DerivationIndex32) throws -> String {
        try "\(productRoot(productId: productId))/\(index.asPathSegment())"
    }

    /// Non-throwing variant for the reserved built-in ids — compile-time constants
    /// that cannot hit the dynamic-input validation.
    public static func builtInAccount(_ productId: String, index: UInt32) -> String {
        "\(root(productId))/\(DerivationIndex32(index: index).asPathSegment())"
    }
}

private extension ProductDerivationPath {
    static func root(_ productId: String) -> String {
        "//product//\(productId)"
    }
}
