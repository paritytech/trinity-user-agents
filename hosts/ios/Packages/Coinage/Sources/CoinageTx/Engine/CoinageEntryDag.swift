import Foundation

/// The live and terminal entries of one pass, indexed for O(1) provenance lookups.
///
/// Built once per pass from the ledger snapshot and the handoff marks, so the rules query precomputed
/// maps instead of re-scanning every entry. Keyed by each asset's stable identifier.
public struct CoinageEntryDag: Sendable {
    public let entries: [CoinageTxEntry]
    private let handedOff: Set<PublicKey>
    private let byId: [CoinageTxId: CoinageTxEntry]
    private let minterByKey: [PublicKey: CoinageTxEntry]
    private let consumersByKey: [PublicKey: [CoinageTxEntry]]

    public init(entries: [CoinageTxEntry], handedOff: Set<PublicKey>) {
        self.entries = entries
        self.handedOff = handedOff

        var ids: [CoinageTxId: CoinageTxEntry] = [:]
        var minters: [PublicKey: CoinageTxEntry] = [:]
        var consumers: [PublicKey: [CoinageTxEntry]] = [:]
        for entry in entries {
            ids[entry.id] = entry
            for output in entry.outputs {
                minters[output.publicKey] = entry
            }
            for input in entry.inputs {
                consumers[input.publicKey, default: []].append(entry)
            }
        }
        byId = ids
        minterByKey = minters
        consumersByKey = consumers
    }

    /// The entry with this id, if the snapshot holds it.
    public func entry(_ id: CoinageTxId) -> CoinageTxEntry? {
        byId[id]
    }

    /// The entry that minted the asset with this identifier, if any.
    public func minter(_ key: PublicKey) -> CoinageTxEntry? {
        minterByKey[key]
    }

    /// Every entry consuming the asset with this identifier, including terminal ones.
    public func consumers(_ key: PublicKey) -> [CoinageTxEntry] {
        consumersByKey[key] ?? []
    }

    /// Every entry that consumes one of `entry`'s outputs.
    public func successors(_ entry: CoinageTxEntry) -> [CoinageTxEntry] {
        var seen: Set<CoinageTxId> = []
        var result: [CoinageTxEntry] = []
        for output in entry.outputs {
            for consumer in consumers(output.publicKey) where seen.insert(consumer.id).inserted {
                result.append(consumer)
            }
        }
        return result
    }

    public func isHandedOff(_ key: PublicKey) -> Bool {
        handedOff.contains(key)
    }
}
