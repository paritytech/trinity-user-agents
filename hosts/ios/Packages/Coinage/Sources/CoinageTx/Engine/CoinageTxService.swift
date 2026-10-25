import AsyncExtensions
import DurableTransactions
import Foundation
import SDKLogger

/// The durability subsystem's coinage face.
public protocol CoinageTxServicing: Sendable {
    /// Registers several transactions atomically under one `groupId` — all commit or none do — then
    /// tracks each. Returns their ids in request order. A within-batch conflict rejects the whole batch
    /// and nothing is registered.
    @discardableResult
    func submitTransactions(
        _ requests: [CoinageTxRequest],
        groupId: CoinageTxGroupId?,
        custody: NativeTransferCustody?,
        authorization: (@Sendable () throws -> Void)?
    ) async throws -> [CoinageTxId]

    func retainNativeTransfer(
        _ custody: NativeTransferCustody, authorization: @escaping @Sendable () throws -> Void
    ) async throws -> any CoinageHandoffCommit

    func retainedNativeTransfer(
        custodyId: String
    ) async throws -> (custody: NativeTransferCustody, handoffCommit: any CoinageHandoffCommit)?

    /// A stream of a submitted entry's status: the current value, then every change. Lets a caller that
    /// must not report success until the chain has — offboarding an external payment — await a terminal
    /// outcome after a fire-and-forget submission.
    func subscribeTransactionStatus(_ id: CoinageTxId) -> AnyAsyncSequence<CoinageTxStatus>

    /// Every entry registered under `groupId`, in registration order — a snapshot read for seeding a
    /// status before subscribing. Empty when the group has never been registered.
    func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry]

    /// A stream of the entries registered under `groupId`: the current set, then every change, in
    /// registration order. Lets a claim watch its whole group settle by `groupId = messageId`.
    func subscribeOperationGroupStatuses(_ groupId: CoinageTxGroupId) -> AnyAsyncSequence<[CoinageTxEntry]>

    /// Provisionally reserves `assets` against being spent again, before their keys reach the transport.
    /// The reservation is released on relaunch unless the returned handle is committed once the carrying
    /// payload is durable — so a payment that fails after this point never freezes the coins.
    func preCommitHandoff(_ assets: [OwnAsset]) async throws -> any CoinageHandoffCommit

    /// Clears the reservations of payments that never became durable. Runs once, on launch.
    func releaseUncommittedHandoffs() async throws
}

public extension CoinageTxServicing {
    @discardableResult
    func submitTransactions(
        _ requests: [CoinageTxRequest],
        groupId: CoinageTxGroupId?
    ) async throws -> [CoinageTxId] {
        try await submitTransactions(requests, groupId: groupId, custody: nil, authorization: nil)
    }

    /// Registers one transaction and starts tracking its extrinsic in the background, returning the
    /// entry's id as soon as it is committed — so the inputs are claimed before this returns, but the
    /// caller does not wait for inclusion. `groupId` labels the operation that registered it (e.g. a
    /// transfer's message id), or `nil` when ungrouped. Status is resolved by the tracker and the
    /// recovery pass; a caller that must await the outcome observes it via
    /// ``subscribeTransactionStatus(_:)``.
    ///
    /// The one-request case of ``submitTransactions(_:groupId:)``.
    @discardableResult
    func submitTransaction(request: CoinageTxRequest, groupId: CoinageTxGroupId?) async throws -> CoinageTxId {
        let ids = try await submitTransactions([request], groupId: groupId)
        guard let id = ids.first else {
            throw TransferStrategyError.submissionFailed(CancellationError())
        }
        return id
    }
}

/// Coinage's view of the durability engine.
///
/// The engine owns the transaction row, the submission watch and recovery; this owns the assets each
/// transaction consumes and mints, and the locks they carry. Registration of the two commits together —
/// the asset rows are written inside the engine's own write transaction — which is what keeps the rule
/// that no extrinsic is ever in flight without a record holding its inputs.
public final class CoinageTxService: CoinageTxServicing {
    private let engine: any DurableTxServicing
    private let ledger: any CoinageAssetLedgerProtocol
    private let logger: SDKLoggerProtocol?
    private let lifecycle: CoinageLifecycle?

    public init(
        engine: any DurableTxServicing,
        ledger: any CoinageAssetLedgerProtocol,
        logger: SDKLoggerProtocol?,
        lifecycle: CoinageLifecycle? = nil
    ) {
        self.engine = engine
        self.ledger = ledger
        self.logger = logger
        self.lifecycle = lifecycle
    }

    @discardableResult
    public func submitTransactions(
        _ requests: [CoinageTxRequest],
        groupId: CoinageTxGroupId?,
        custody: NativeTransferCustody?,
        authorization: (@Sendable () throws -> Void)?
    ) async throws -> [CoinageTxId] {
        let operation = try lifecycle?.captureOperation()
        let assets = requests.map { CoinageAssetRegistration(inputs: $0.inputs, outputs: $0.outputs) }
        guard assets.allSatisfy({ !$0.isEmpty }) else {
            throw CoinageTxError.emptyEntry
        }

        logger?.debug("Submitting \(requests.count) coinage request(s) groupId: \(String(describing: groupId))")

        do {
            if custody != nil { try Task.checkCancellation() }
            return try await engine.submitTransactions(
                domain: .coinage,
                requests: requests.map { DurableTxRequest(builder: $0.builder, origin: $0.origin) },
                groupId: groupId
            ) { [ledger, lifecycle] scope, ids in
                let register = {
                    if custody != nil { try Task.checkCancellation() }
                    try ledger.registerAssets(assets, for: ids, custody: custody, authorization: authorization, in: scope)
                }
                if let lifecycle, let operation {
                    try lifecycle.withEffect(operation, register)
                } else {
                    try register()
                }
            }
        } catch let error as DurableTxError {
            throw CoinageTxError(durableTxError: error) ?? error
        }
    }

    public func retainNativeTransfer(
        _ custody: NativeTransferCustody, authorization: @escaping @Sendable () throws -> Void
    ) async throws -> any CoinageHandoffCommit {
        let operation = try lifecycle?.captureOperation()
        try Task.checkCancellation()
        try await ledger.retainNativeTransfer(custody) { [lifecycle] in
            if let lifecycle, let operation {
                try lifecycle.withEffect(operation, authorization)
            } else {
                try authorization()
            }
        }
        return StoreHandoffCommit(assets: custody.assets, ledger: ledger)
    }

    public func retainedNativeTransfer(
        custodyId: String
    ) async throws -> (custody: NativeTransferCustody, handoffCommit: any CoinageHandoffCommit)? {
        let operation = try lifecycle?.captureOperation()
        guard let custody = try await ledger.retainedNativeTransfer(custodyId: custodyId) else { return nil }
        if let lifecycle, let operation { try lifecycle.check(operation) }
        return (custody, StoreHandoffCommit(assets: custody.assets, ledger: ledger))
    }

    public func subscribeTransactionStatus(_ id: CoinageTxId) -> AnyAsyncSequence<CoinageTxStatus> {
        engine.subscribeTransactionStatus(id)
    }

    public func getOperationGroupStatuses(_ groupId: CoinageTxGroupId) async throws -> [CoinageTxEntry] {
        try await ledger.getOperationGroupStatuses(groupId)
    }

    public func subscribeOperationGroupStatuses(
        _ groupId: CoinageTxGroupId
    ) -> AnyAsyncSequence<[CoinageTxEntry]> {
        ledger.subscribeOperationGroupStatuses(groupId)
    }

    /// Rejects live claims and already-reserved recipients inside the same transaction as the mark.
    public func preCommitHandoff(_ assets: [OwnAsset]) async throws -> any CoinageHandoffCommit {
        let operation = try lifecycle?.captureOperation()
        let keys = Set(assets.map(\.publicKey))
        try await ledger.precommitHandOff(assets) { [lifecycle] context in
            let validate = { try CoinageTxRegistrationValidator().validateHandoff(keys, transaction: context) }
            if let lifecycle, let operation {
                try lifecycle.withEffect(operation, validate)
            } else {
                try validate()
            }

            let handedOff = try context.filterHandedOff(keys)
            if let key = handedOff.first {
                throw CoinageTxError.handoffOfHandedOffAsset(key.toHex())
            }
        }
        return StoreHandoffCommit(assets: assets, ledger: ledger)
    }

    public func releaseUncommittedHandoffs() async throws {
        try await ledger.releaseUncommittedHandoffs()
    }
}
