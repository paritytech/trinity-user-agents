import Foundation

public typealias IncomingPaymentId = String

/// What a top-up draws its funds from, as the persisted bytes rather than anything resolved.
/// NOTE: Stored in the secure storage as encoded json. Only append, don't change the fields
public enum IncomingPaymentSourceDescriptor: Equatable, Sendable, Codable {
    /// The full product-account derivation path (`//product//{productId}/{index}`), so the path alone
    /// names the account across products.
    case productAccount(derivationPath: String)
    case privateKey(secretKey: Data)
    case coins(secretKeys: [Data])
}

/// Persisted discriminator for a descriptor (the tag the secret store serialises alongside the bytes).
public enum IncomingPaymentSourceKind: String, Sendable, Equatable {
    case productAccount
    case privateKey
    case coins
}

public extension IncomingPaymentSourceDescriptor {
    var kind: IncomingPaymentSourceKind {
        switch self {
        case .productAccount: .productAccount
        case .privateKey: .privateKey
        case .coins: .coins
        }
    }

    /// Whether two sources are the same money and so cannot be claimed at once.
    ///
    /// A product-account path embeds its product, so equal paths are the same account. A private key
    /// is the same only as itself. Coin keys are the money rather than a way to reach it, so a single
    /// key in common is enough — both top-ups would submit a claim for that coin and one would be refused.
    func drawsOnSameFunds(as other: IncomingPaymentSourceDescriptor) -> Bool {
        switch (self, other) {
        case let (.productAccount(lhs), .productAccount(rhs)):
            lhs == rhs
        case let (.privateKey(lhs), .privateKey(rhs)):
            lhs == rhs
        case let (.coins(lhs), .coins(rhs)):
            !Set(lhs).isDisjoint(with: Set(rhs))
        default:
            false
        }
    }
}
