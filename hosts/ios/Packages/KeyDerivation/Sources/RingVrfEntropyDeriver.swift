import Foundation

/// Ring-VRF key derivation: a keyed-hash chain rooted at
/// `hash(root_entropy, "ring-vrf")` with path `//{domain}//{index}`,
/// where the domain is a product identity (e.g. `peopl.dot` for personhood keys).
public final class RingVrfEntropyDeriver: BandersnatchEntropyDeriving {
    static let rootKey = Data("ring-vrf".utf8)

    private let domain: String
    private let index: UInt32

    public init(domain: String, index: UInt32) {
        self.domain = domain
        self.index = index
    }

    public func deriveEntropy(from rootEntropy: Data) throws -> Data {
        let treeRoot = try KeyedHashChainDeriver.deriveRoot(entropy: rootEntropy, rootKey: Self.rootKey)
        let indexSegment = KeyedHashChainSegment.index(DerivationIndex32(index: index))
        return try KeyedHashChainDeriver.derive(
            parent: treeRoot,
            segments: [.string(domain), indexSegment]
        )
    }
}
