import DurableTransactions
import Foundation

/// Turns one entry plus the pass's pinned heads into the ``ChainEvidence`` the rules read, built on the
/// reader's three-valued asset reads: a present voucher already carries its unload state, which is the
/// alias signal the rules need.
public struct CoinageEvidenceCollector: Sendable {
    public init() {}

    public func collect(
        entry: CoinageTxEntry,
        reader: any CoinageStateReading,
        heads: ChainHeads
    ) async -> ChainEvidence {
        let finalized = heads.finalized
        let best = heads.best

        async let inputsAtFinalized = reader.readInputs(entry.inputs, at: finalized)
        async let inputsAtBest = reader.readInputs(entry.inputs, at: best)
        async let outputsAtFinalized = reader.readOutputs(entry.outputs, at: finalized)
        async let outputsAtBest = reader.readOutputs(entry.outputs, at: best)

        let inputKeys = entry.inputs.map(\.publicKey)
        let outputKeys = entry.outputs.map(\.publicKey)

        let (presenceF, aliasF) = await maps(
            inputKeys: inputKeys,
            inputReads: inputsAtFinalized,
            outputKeys: outputKeys,
            outputReads: outputsAtFinalized
        )
        let (presenceB, aliasB) = await maps(
            inputKeys: inputKeys,
            inputReads: inputsAtBest,
            outputKeys: outputKeys,
            outputReads: outputsAtBest
        )

        return ChainEvidence(
            finalized: finalized,
            best: best,
            presenceAtFinalized: presenceF,
            presenceAtBest: presenceB,
            aliasAtFinalized: aliasF,
            aliasAtBest: aliasB
        )
    }
}

private extension CoinageEvidenceCollector {
    func maps(
        inputKeys: [PublicKey],
        inputReads: [ReadResult<AssetPresence>],
        outputKeys: [PublicKey],
        outputReads: [ReadResult<AssetPresence>]
    ) -> (presence: [PublicKey: ChainPresence], alias: [PublicKey: AliasRead]) {
        var presence: [PublicKey: ChainPresence] = [:]
        var alias: [PublicKey: AliasRead] = [:]
        for (key, read) in Array(zip(inputKeys, inputReads)) + Array(zip(outputKeys, outputReads)) {
            presence[key] = Self.presence(read)
            alias[key] = Self.alias(read)
        }
        return (presence, alias)
    }

    static func presence(_ read: ReadResult<AssetPresence>) -> ChainPresence {
        switch read {
        case .present: .present
        case .absent: .absent
        case .failedRead: .unknown
        }
    }

    /// A coin has no alias to be marked, so a present coin reads not-unloaded; no rule consults it.
    static func alias(_ read: ReadResult<AssetPresence>) -> AliasRead {
        switch read {
        case .present(.coin),
             .present(.voucher(.notUnloaded)): .notUnloaded
        case .present(.voucher(.unloaded)): .unloaded
        case .present(.voucher(.unknown)),
             .absent,
             .failedRead: .unknown
        }
    }
}
