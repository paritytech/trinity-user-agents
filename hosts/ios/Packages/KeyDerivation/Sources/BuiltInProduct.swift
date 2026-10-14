import Foundation

/// Governance-reserved product identities for built-in app features.
/// Only `dim2` and `personhood` are wired today; the rest are declared for
/// when those features migrate to the product scheme. Coinage keeps its own
/// derivation layout for now.
public enum BuiltInProduct {
    /// Game (DIM2). `@Sendable` so the value of this function can be stored in a
    /// `Sendable` type, which is how `HostPlacedProducts` names its backing product.
    @Sendable public static func dim2(for tld: String) -> String {
        product(for: "dim2", tld: tld)
    }

    /// PoI (DIM1).
    public static func poi(for tld: String) -> String {
        product(for: "poi", tld: tld)
    }

    /// Funding.
    public static func fund(for tld: String) -> String {
        product(for: "fund", tld: tld)
    }

    /// Public light person identity.
    public static func uid(for tld: String) -> String {
        product(for: "uid", tld: tld)
    }

    /// Personhood — ring-VRF key domain for full/light person keys.
    public static func personhood(for tld: String) -> String {
        product(for: "peopl", tld: tld)
    }

    static func product(for name: String, tld: String) -> String {
        "\(name).\(tld)"
    }
}
