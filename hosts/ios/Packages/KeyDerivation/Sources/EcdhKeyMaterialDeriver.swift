import Foundation

public protocol EcdhKeyMaterialDeriving {
    func deriveKeyMaterial(for domain: String) throws -> Data
}

/// ECDH key material derivation: a keyed-hash chain rooted at
/// `hash(root_entropy, "ecdh")` with path `//{domain}`. The mapping of the
/// 32-byte material to a concrete key type is up to the consumer.
public final class EcdhKeyMaterialDeriver {
    static let rootKey = Data("ecdh".utf8)

    private let entropyManager: RootEntropyManaging

    public init(entropyManager: RootEntropyManaging) {
        self.entropyManager = entropyManager
    }
}

extension EcdhKeyMaterialDeriver: EcdhKeyMaterialDeriving {
    public func deriveKeyMaterial(for domain: String) throws -> Data {
        let rootEntropy = try entropyManager.fetchRootEntropy()
        let treeRoot = try KeyedHashChainDeriver.deriveRoot(entropy: rootEntropy, rootKey: Self.rootKey)
        return try KeyedHashChainDeriver.derive(parent: treeRoot, segments: [.string(domain)])
    }
}
