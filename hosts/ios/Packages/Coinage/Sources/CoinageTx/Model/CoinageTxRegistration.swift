import DurableTransactions
import Foundation

/// The assets one transaction consumes and mints, as registration supplies them to the coinage half of
/// the ledger.
public struct CoinageAssetRegistration: Sendable, Equatable {
    public let inputs: [CoinageTxInput]
    public let outputs: [OwnAsset]

    public init(inputs: [CoinageTxInput], outputs: [OwnAsset]) {
        self.inputs = inputs
        self.outputs = outputs
    }

    /// Nothing to lock and nothing to look for on chain, so the rules could never decide it.
    public var isEmpty: Bool {
        inputs.isEmpty && outputs.isEmpty
    }
}

/// A coinage transaction ready to be recorded: the engine's registration (hash and window) plus the
/// assets it consumes and mints. Carries no id — the repository mints the ``CoinageTxId`` inside the
/// write transaction.
public struct CoinageTxRegistration: Sendable, Equatable {
    public let txHash: Data
    public let checkpoint: BlockRef
    public let mortalityBlocks: UInt32
    public let groupId: CoinageTxGroupId?
    public let inputs: [CoinageTxInput]
    public let outputs: [OwnAsset]

    public init(
        txHash: Data,
        checkpoint: BlockRef,
        mortalityBlocks: UInt32,
        groupId: CoinageTxGroupId?,
        inputs: [CoinageTxInput],
        outputs: [OwnAsset]
    ) {
        self.txHash = txHash
        self.checkpoint = checkpoint
        self.mortalityBlocks = mortalityBlocks
        self.groupId = groupId
        self.inputs = inputs
        self.outputs = outputs
    }
}

public extension CoinageTxRegistration {
    /// The engine's half: what the shared ledger row holds.
    var durable: DurableTxRegistration {
        DurableTxRegistration(
            domainId: .coinage,
            groupId: groupId,
            txHash: txHash,
            checkpoint: checkpoint,
            mortalityBlocks: mortalityBlocks
        )
    }

    /// Coinage's half: what its own rows hold.
    var assets: CoinageAssetRegistration {
        CoinageAssetRegistration(inputs: inputs, outputs: outputs)
    }

    /// Builds the entry a store holds — `id` minted by the store, `sequence` the next in order, status
    /// `.pending`.
    func makeEntry(id: CoinageTxId, sequence: Int64) -> CoinageTxEntry {
        CoinageTxEntry(entry: durable.makeEntry(id: id, sequence: sequence), inputs: inputs, outputs: outputs)
    }
}
