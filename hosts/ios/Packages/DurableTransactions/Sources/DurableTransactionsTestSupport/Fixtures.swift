import DurableTransactions
import ExtrinsicService
import Foundation
import SubstrateSdk

public extension TxDomainId {
    /// A domain for tests that need one and do not care which.
    static let test = TxDomainId("test")
}

public extension BlockRef {
    /// Block `number` with a hash derived from it, so distinct numbers stay distinguishable.
    static func fixture(_ number: UInt32) -> BlockRef {
        BlockRef(number: number, hash: Data([UInt8(truncatingIfNeeded: number)]))
    }
}

public extension DurableTxRegistration {
    /// A registration checkpointed at block 100 with a 60-block window and a fixed `txHash` — the
    /// baseline the registrar suites vary from.
    static func fixture(
        domainId: TxDomainId = .test,
        checkpoint: BlockRef = .fixture(100),
        groupId: DurableTxGroupId? = nil,
        txHash: Data = Data(repeating: 0xAB, count: 32)
    ) -> DurableTxRegistration {
        DurableTxRegistration(
            domainId: domainId,
            groupId: groupId,
            txHash: txHash,
            checkpoint: checkpoint,
            mortalityBlocks: 60
        )
    }
}

public extension DurableTxEntry {
    /// A pending entry checkpointed at block 100 with a 60-block window — the baseline the durability
    /// suites vary from.
    static func fixture(
        id: DurableTxId = UUID(),
        domainId: TxDomainId = .test,
        checkpoint: BlockRef = .fixture(100),
        mortality: UInt32 = 60,
        successDetectedAt: BlockRef? = nil,
        status: DurableTxStatus = .pending,
        groupId: DurableTxGroupId? = nil,
        txHash: Data? = nil
    ) -> DurableTxEntry {
        DurableTxEntry(
            id: id,
            domainId: domainId,
            groupId: groupId,
            txHash: txHash ?? Data("tx\(id.uuidString)".utf8),
            checkpoint: checkpoint,
            mortality: mortality,
            successDetectedAt: successDetectedAt,
            status: status
        )
    }
}

public extension ExtrinsicBuiltModel {
    /// A mortal built extrinsic anchored at `checkpoint` with a `mortalityBlocks` window, so
    /// ``DurableTxAttempt/init(from:)`` can read an attempt off it. `payload` varies the bytes, and with
    /// them the hash: two models with different payloads are two different attempts.
    static func fixture(
        payload: String,
        checkpoint: BlockRef = .fixture(100),
        mortalityBlocks: UInt64 = 60
    ) -> ExtrinsicBuiltModel {
        ExtrinsicBuiltModel(
            extrinsic: Data(payload.utf8).toHex(includePrefix: true),
            sender: .none,
            mortality: .mortal(
                MortalExtrinsic(
                    era: .mortal(period: mortalityBlocks, phase: 0),
                    anchorBlock: BlockNumberWithHash(
                        blockNumber: checkpoint.number,
                        blockHash: checkpoint.hash
                    )
                )
            )
        )
    }
}
