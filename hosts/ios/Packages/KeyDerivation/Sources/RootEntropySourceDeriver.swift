import Foundation
import SubstrateSdk

/// Computes `rootEntropySource` — the first layer of the deterministic entropy
/// derivation defined in TrUAPI RFC-7. The argument is the raw BIP-39 entropy
/// bytes of the root account, NOT the 64-byte PBKDF2-derived seed.
public protocol RootEntropySourceDeriving {
    func deriveRootEntropySource() throws -> Data
}

public final class RootEntropySourceDeriver: RootEntropySourceDeriving {
    private static let domainSeparator = Data("product-entropy-derivation".utf8)

    private let entropyManager: any RootEntropyManaging

    public init(entropyManager: any RootEntropyManaging) {
        self.entropyManager = entropyManager
    }

    public func deriveRootEntropySource() throws -> Data {
        let rootAccountSecret = try entropyManager.fetchRootEntropy()
        return try rootAccountSecret.blake2b32WithKey(Self.domainSeparator)
    }
}
