import ExtrinsicService
import Foundation

/// Names the requests of one submission that must be built together so their nonces are sequential —
/// one spending another's output. Every request under a key must carry the same signing origin; the
/// batch is built with the first one's.
public struct DurableTxBatchKey: Hashable, Sendable {
    public let rawValue: String

    public init(_ rawValue: String) {
        self.rawValue = rawValue
    }
}

/// One transaction to build, register and submit. A batch of these registers atomically under one
/// ``DurableTxGroupId``; requests sharing a ``DurableTxBatchKey`` are built together so their nonces are
/// sequential, and a request without one is built on its own.
public struct DurableTxRequest {
    public let builder: ExtrinsicBuilderClosure
    public let origin: any ExtrinsicOriginDefining
    public let batchKey: DurableTxBatchKey?

    public init(
        builder: @escaping ExtrinsicBuilderClosure,
        origin: any ExtrinsicOriginDefining,
        batchKey: DurableTxBatchKey? = nil
    ) {
        self.builder = builder
        self.origin = origin
        self.batchKey = batchKey
    }
}

/// Failures raised by the durable transaction engine.
public enum DurableTxError: Error, Equatable {
    /// The built extrinsic is immortal, so it carries no era window to recover it against.
    case notMortal
    /// The transaction is not in the store.
    case entryNotFound(DurableTxId)
    /// A pinned chain view could not be read, so the pass cannot run.
    case chainViewUnavailable
    /// No ``TxCompletionOracle`` is registered for the domain, so the engine cannot tell which chain its
    /// transactions live on.
    case unregisteredDomain(TxDomainId)
    /// The builder returned fewer extrinsics than requested.
    case buildIncomplete
    /// A domain store was handed a registration scope opened by a different store technology.
    case foreignRegistrationScope
}
