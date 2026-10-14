import CoreData
import DurableTransactions
import Foundation
import Operation_iOS
import SubstrateSdk
import SubstrateSdkExt

/// Maps the engine's ``DurableTxEntry`` to `CDDurableTx` — the domain-neutral fields only. A domain's
/// rows hang off the same entity through their own relations and are written by that domain's store.
final class DurableTxMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = DurableTxEntry
    typealias CoreDataEntity = CDDurableTx

    var entityIdentifierFieldName: String { #keyPath(CDDurableTx.identifier) }

    func transform(entity: CDDurableTx) throws -> DurableTxEntry {
        guard let identifier = entity.identifier, let id = UUID(uuidString: identifier) else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDDurableTx.identifier))
        }
        guard let status = DurableTxStatus(rawValue: Int(entity.status)) else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDDurableTx.status))
        }
        guard let domainId = entity.domainId else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDDurableTx.domainId))
        }
        guard let createdAt = entity.createdAt else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDDurableTx.createdAt))
        }

        let successDetectedAt: BlockRef? =
            if let successHash = entity.successHash, let successNumber = entity.successNumber {
                try BlockRef(number: successNumber.uint32Value, hash: successHash.fromHex())
            } else {
                nil
            }

        return try DurableTxEntry(
            id: id,
            domainId: TxDomainId(domainId),
            sequence: entity.sequence,
            groupId: entity.groupId,
            attempt: Self.attempt(of: entity),
            successDetectedAt: successDetectedAt,
            status: status,
            createdAt: createdAt
        )
    }

    /// The attempt a row carries, or `nil` when it has none.
    ///
    /// A row scheduled but not yet built stores NULL in all three columns. A row that stores only some
    /// of them is not a half-attempt to be guessed at — it is unreadable, and `nil` keeps it out of
    /// every rule rather than inventing a window for it.
    static func attempt(of entity: CDDurableTx) throws -> DurableTxAttempt? {
        guard let txHashString = entity.txHash,
              let checkpointHash = entity.checkpointHash,
              let checkpointNumber = entity.checkpointNumber
        else {
            return nil
        }

        return try DurableTxAttempt(
            txHash: txHashString.fromHex(),
            checkpoint: BlockRef(number: checkpointNumber.uint32Value, hash: checkpointHash.fromHex()),
            mortalityBlocks: UInt32(bitPattern: entity.mortality)
        )
    }

    /// The policy that may build this row again, if it has one.
    static func policy(of entity: CDDurableTx) -> SubmissionPolicy? {
        guard let id = entity.submissionPolicyId, let params = entity.submissionPolicyParams else {
            return nil
        }

        return SubmissionPolicy(id: SubmissionPolicyId(id), params: params)
    }

    /// Writes the attempt columns, or clears them for a row that has none yet.
    static func apply(attempt: DurableTxAttempt?, to entity: CDDurableTx) {
        entity.txHash = attempt?.txHash.toHex()
        entity.checkpointHash = attempt?.checkpoint.hash.toHex()
        entity.checkpointNumber = attempt.map { NSNumber(value: $0.checkpoint.number) }
        entity.mortality = Int32(bitPattern: attempt?.mortalityBlocks ?? 0)
    }

    static func apply(policy: SubmissionPolicy?, to entity: CDDurableTx) {
        entity.submissionPolicyId = policy?.id.rawValue
        entity.submissionPolicyParams = policy?.params
    }

    func populate(entity: CDDurableTx, from model: DurableTxEntry, using _: NSManagedObjectContext) throws {
        entity.identifier = model.identifier
        entity.domainId = model.domainId.rawValue
        entity.sequence = model.sequence
        entity.groupId = model.groupId
        entity.createdAt = model.createdAt

        Self.apply(attempt: model.attempt, to: entity)
        Self.apply(status: model.status, successDetectedAt: model.successDetectedAt, to: entity)
    }

    /// The status write alone — what a verdict changes. Kept separate from a full populate so the
    /// compare-and-set touches nothing else.
    static func apply(status: DurableTxStatus, successDetectedAt: BlockRef?, to entity: CDDurableTx) {
        entity.status = Int16(status.rawValue)
        entity.successHash = successDetectedAt?.hash.toHex()
        entity.successNumber = successDetectedAt.map { NSNumber(value: $0.number) }
    }
}
