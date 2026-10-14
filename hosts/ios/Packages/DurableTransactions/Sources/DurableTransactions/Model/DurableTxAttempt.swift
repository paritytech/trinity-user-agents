import ExtrinsicService
import Foundation
import SubstrateSdk
import SubstrateSdkExt

/// One attempt at a transaction: the bytes' hash and the window they can land in.
///
/// A transaction may have several attempts over its life — a rebuild replaces this triple in place and
/// keeps the row, so the domain's locked inputs and outputs stay attached to the same id.
public struct DurableTxAttempt: Sendable, Equatable {
    public let txHash: Data
    public let checkpoint: BlockRef
    public let mortalityBlocks: UInt32

    public init(txHash: Data, checkpoint: BlockRef, mortalityBlocks: UInt32) {
        self.txHash = txHash
        self.checkpoint = checkpoint
        self.mortalityBlocks = mortalityBlocks
    }
}

public extension DurableTxAttempt {
    /// The last block this attempt can still execute in. Widened so a checkpoint near `UInt32.max`
    /// cannot overflow.
    var mortalityEnd: UInt64 {
        UInt64(checkpoint.number) + UInt64(mortalityBlocks)
    }

    /// True when the extrinsic can no longer be included: `finalizedNumber` is past the last block of
    /// the mortality window.
    func isWindowClosed(atFinalized finalizedNumber: UInt32) -> Bool {
        UInt64(finalizedNumber) > mortalityEnd
    }

    /// Reads the attempt off a built extrinsic.
    ///
    /// Both the checkpoint and the mortality window come from the extrinsic's own `CheckMortality` era —
    /// the window the runtime will actually enforce, which is exactly what the body search must cover —
    /// rather than being re-derived from the chain: re-deriving from a head read at registration time can
    /// name a different block once the head has crossed a period boundary. The `txHash` is the up-front
    /// hash of the built extrinsic, so a transaction is resolvable by the search even before tracking
    /// records anything.
    init(from model: ExtrinsicBuiltModel) throws {
        guard let anchor = model.mortalityAnchorBlock, let period = model.mortalityPeriod else {
            throw DurableTxError.notMortal
        }

        try self.init(
            txHash: model.extrinsic.fromHex().blake2b32(),
            checkpoint: BlockRef(number: anchor.blockNumber, hash: anchor.blockHash),
            mortalityBlocks: UInt32(period)
        )
    }
}
