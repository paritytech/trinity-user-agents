import Foundation
import TrUAPIHost

/// Governance-reserved product identities for built-in app features.
/// Only `dim2` and `personhood` are wired today; the rest are declared for
/// when those features migrate to the product scheme. Coinage keeps its own
/// derivation layout for now.
public enum BuiltInProduct {
    /// Game (DIM2).
    public static func dim2(for tld: String) throws -> ProductId {
        try privilegedProductId(product: .game, networkSuffix: tld)
    }

    /// PoI (DIM1).
    public static func poi(for tld: String) throws -> ProductId {
        try productIdFromName(name: "poi", networkSuffix: tld)
    }

    /// Funding.
    public static func fund(for tld: String) throws -> ProductId {
        try productIdFromName(name: "fund", networkSuffix: tld)
    }

    /// Chat — per-chat statement and upload accounts.
    public static func chat(for tld: String) throws -> ProductId {
        try productIdFromName(name: "chat", networkSuffix: tld)
    }

    /// Personhood — ring-VRF key domain for full/light person keys; account 0 is the main
    /// account (RFC-0028).
    public static func personhood(for tld: String) throws -> ProductId {
        try privilegedProductId(product: .personhood, networkSuffix: tld)
    }
}
