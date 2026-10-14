import Foundation
import SubstrateSdk

public enum KeyedHashChainError: Error, Equatable {
    case invalidSegment(String)
}

/// A segment of a keyed-hash derivation path. All junctions are hard;
/// string segments use the standard substrate chain-code normalization,
/// index segments use the 32-byte index directly.
public enum KeyedHashChainSegment {
    case string(String)
    case index(DerivationIndex32)
}

/// Hard-only keyed-hash HDKD for the ring-VRF and ECDH trees:
/// `root = hash(root_entropy, rootKey)`, `child = hash(parent, chain_code)`,
/// where `hash(data, key)` is keyed BLAKE2b-256.
public enum KeyedHashChainDeriver {
    public static func deriveRoot(entropy: Data, rootKey: Data) throws -> Data {
        try entropy.blake2b32WithKey(rootKey)
    }

    public static func derive(parent: Data, segments: [KeyedHashChainSegment]) throws -> Data {
        try segments.reduce(parent) { accumulated, segment in
            try accumulated.blake2b32WithKey(segment.chainCode())
        }
    }
}

extension KeyedHashChainSegment {
    func chainCode() throws -> Data {
        switch self {
        case let .string(value):
            guard !value.isEmpty, !value.contains("/") else {
                throw KeyedHashChainError.invalidSegment(value)
            }

            guard let chaincode = try SubstrateJunctionFactory()
                .parse(path: "//\(value)").chaincodes.first else {
                throw KeyedHashChainError.invalidSegment(value)
            }

            return chaincode.data
        case let .index(index):
            return index.bytes
        }
    }
}
