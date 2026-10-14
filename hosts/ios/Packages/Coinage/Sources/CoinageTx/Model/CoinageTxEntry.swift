import DurableTransactions
import Foundation
import Operation_iOS

/// Coinage's transactions are rows of the shared durability ledger, so these name the engine's types
/// rather than parallel ones. Callers keep the coinage-flavoured spelling; there is one set of values
/// underneath.
public typealias CoinageTxId = DurableTxId
public typealias CoinageTxGroupId = DurableTxGroupId
public typealias CoinageTxStatus = DurableTxStatus

public extension TxDomainId {
    /// Coinage's rows of the shared ledger.
    static let coinage = TxDomainId("coinage")
}

/// One tracked transaction as coinage sees it: the engine's row joined to the assets it consumes and
/// mints.
///
/// The engine half is domain-neutral and lives in the shared ledger; the asset half is coinage's alone.
/// Deliberately operation-agnostic: every rule quantifies over `inputs` and `outputs`, so all tracked
/// calls share one resolution path and no per-operation dispatch exists.
public struct CoinageTxEntry: Sendable, Equatable {
    public let entry: DurableTxEntry
    public let inputs: [CoinageTxInput]
    public let outputs: [OwnAsset]

    public init(entry: DurableTxEntry, inputs: [CoinageTxInput], outputs: [OwnAsset]) {
        self.entry = entry
        self.inputs = inputs
        self.outputs = outputs
    }

    public init(
        id: CoinageTxId = UUID(),
        sequence: Int64 = 0,
        inputs: [CoinageTxInput],
        outputs: [OwnAsset],
        groupId: CoinageTxGroupId? = nil,
        txHash: Data,
        checkpoint: BlockRef,
        mortality: UInt32,
        successDetectedAt: BlockRef? = nil,
        status: CoinageTxStatus = .pending,
        createdAt: Date = Date()
    ) {
        entry = DurableTxEntry(
            id: id,
            domainId: .coinage,
            sequence: sequence,
            groupId: groupId,
            txHash: txHash,
            checkpoint: checkpoint,
            mortality: mortality,
            successDetectedAt: successDetectedAt,
            status: status,
            createdAt: createdAt
        )
        self.inputs = inputs
        self.outputs = outputs
    }

    public var id: CoinageTxId { entry.id }
    public var sequence: Int64 { entry.sequence }
    public var groupId: CoinageTxGroupId? { entry.groupId }
    /// The bytes currently in flight and the window they can land in; `nil` while the transaction
    /// waits to be built.
    public var attempt: DurableTxAttempt? { entry.attempt }

    public var txHash: Data? { entry.txHash }
    public var checkpoint: BlockRef? { entry.checkpoint }
    public var mortality: UInt32? { entry.mortality }
    public var successDetectedAt: BlockRef? { entry.successDetectedAt }
    public var status: CoinageTxStatus { entry.status }
    public var createdAt: Date { entry.createdAt }
}

extension CoinageTxEntry: Operation_iOS.Identifiable {
    public var identifier: String {
        entry.identifier
    }
}

public extension CoinageTxEntry {
    /// A copy with `status` replaced — the entry is immutable, so a status write rebuilds it.
    func withStatus(_ status: CoinageTxStatus) -> CoinageTxEntry {
        CoinageTxEntry(entry: entry.withStatus(status), inputs: inputs, outputs: outputs)
    }

    /// A copy with `successDetectedAt` replaced (`nil` clears the record).
    func withSuccessDetectedAt(_ block: BlockRef?) -> CoinageTxEntry {
        CoinageTxEntry(entry: entry.withSuccessDetectedAt(block), inputs: inputs, outputs: outputs)
    }

    /// A copy with `sequence` replaced — for a store assigning the order on insert.
    func withSequence(_ sequence: Int64) -> CoinageTxEntry {
        CoinageTxEntry(entry: entry.withSequence(sequence), inputs: inputs, outputs: outputs)
    }

    /// True when the extrinsic can no longer be included: `finalizedNumber` is past the last block of
    /// the entry's mortality window.
    ///
    /// A transaction with no attempt has no window, so nothing can have closed on it — it is waiting to
    /// be built, not running out of time.
    func isWindowClosed(atFinalized finalizedNumber: UInt32) -> Bool {
        entry.attempt?.isWindowClosed(atFinalized: finalizedNumber) ?? false
    }
}

public extension [CoinageTxEntry] {
    /// Every peer-sent coin these entries name as an input, whatever each entry settled on.
    ///
    /// Answers "which of these coins does the ledger already hold a claim for" — a coin absent from
    /// this set has no claim recorded at all, which is work still to do rather than a settled outcome.
    func receivedPublicKeys() -> Set<PublicKey> {
        var keys: Set<PublicKey> = []
        for entry in self {
            for input in entry.inputs {
                if case let .coin(.received(publicKey)) = input {
                    keys.insert(publicKey)
                }
            }
        }
        return keys
    }

    func finalizedSuccess() -> [CoinageTxEntry] {
        filter { $0.status == .finalizedSuccess }
    }

    func outputPublicKeys() -> Set<PublicKey> {
        Set(flatMap { $0.outputs.map(\.publicKey) })
    }
}
