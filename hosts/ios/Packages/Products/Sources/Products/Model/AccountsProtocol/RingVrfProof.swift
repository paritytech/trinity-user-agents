import Foundation

public struct ContextualAlias: Hashable {
    public let context: Data
    public let alias: Data

    public init(context: Data, alias: Data) {
        self.context = context
        self.alias = alias
    }
}

/// RFC-0004 `create_proof` result: the proof plus the values downstream consumers
/// (precompiles, transaction extensions) need without a separate lookup.
public struct RingVrfProof: Hashable {
    public let proof: Data
    public let contextualAlias: ContextualAlias
    public let ringIndex: UInt32
    public let ringRevision: UInt32

    public init(proof: Data, contextualAlias: ContextualAlias, ringIndex: UInt32, ringRevision: UInt32) {
        self.proof = proof
        self.contextualAlias = contextualAlias
        self.ringIndex = ringIndex
        self.ringRevision = ringRevision
    }
}
