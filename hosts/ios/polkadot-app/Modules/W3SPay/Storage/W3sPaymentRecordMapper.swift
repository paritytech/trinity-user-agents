import Coinage
import CoreData
import Operation_iOS
import SubstrateSdk

final class W3sPaymentRecordMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = W3sPaymentRecord
    typealias CoreDataEntity = CDPaymentRecord

    var entityIdentifierFieldName: String { #keyPath(CDPaymentRecord.paymentId) }

    func transform(entity: CDPaymentRecord) throws -> W3sPaymentRecord {
        guard let paymentId = entity.paymentId,
              let recipientTopic = entity.recipientTopic,
              let merchantPublicKey = entity.merchantPublicKey,
              let amountString = entity.amountString,
              let chainAssetId = entity.chainAssetId,
              let memoData = entity.memoData,
              let createdAt = entity.createdAt,
              let updatedAt = entity.updatedAt
        else {
            throw W3sPaymentRecordMapperError.missingRequiredField
        }

        let decoder = try ScaleDecoder(data: memoData)
        let memo = try TransferMemo(scaleDecoder: decoder)
        let status = W3sPaymentRecord.Status(
            rawValue: entity.status,
            reason: entity.failureReason
        )

        let submitBlock: BlockNumber? = UInt32(bitPattern: entity.submittedAtBlock)

        return W3sPaymentRecord(
            paymentId: paymentId,
            recipientTopic: recipientTopic,
            merchantName: entity.merchantName,
            merchantPublicKey: merchantPublicKey,
            amountString: amountString,
            chainAssetId: chainAssetId,
            memo: memo,
            submittedAtBlock: submitBlock,
            createdAt: createdAt,
            updatedAt: updatedAt,
            status: status
        )
    }

    func populate(
        entity: CDPaymentRecord,
        from model: W3sPaymentRecord,
        using _: NSManagedObjectContext
    ) throws {
        entity.paymentId = model.paymentId
        entity.recipientTopic = model.recipientTopic
        entity.merchantName = model.merchantName
        entity.merchantPublicKey = model.merchantPublicKey
        entity.amountString = model.amountString
        entity.chainAssetId = model.chainAssetId
        entity.memoData = try model.memo.scaleEncoded()
        entity.submittedAtBlock = model.submittedAtBlock.map { Int32(bitPattern: $0) } ?? 0
        entity.createdAt = model.createdAt
        entity.updatedAt = model.updatedAt
        entity.status = model.status.rawValue
        entity.failureReason = model.status.failureReason
    }
}

/// Partial mapper that updates only mutable fields on an existing record.
///
/// Use for status transitions — avoids re-writing immutable fields like paymentId,
/// recipient, amount, etc.
final class W3sPaymentStatusMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case noExistingEntity
    }

    typealias DataProviderModel = W3sPaymentRecord
    typealias CoreDataEntity = CDPaymentRecord

    var entityIdentifierFieldName: String { #keyPath(CDPaymentRecord.paymentId) }

    func transform(entity: CDPaymentRecord) throws -> W3sPaymentRecord {
        try W3sPaymentRecordMapper().transform(entity: entity)
    }

    func populate(
        entity: CDPaymentRecord,
        from model: W3sPaymentRecord,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.paymentId != nil else {
            throw MappingError.noExistingEntity
        }
        entity.status = model.status.rawValue
        entity.failureReason = model.status.failureReason
        entity.updatedAt = model.updatedAt
    }
}

private enum W3sPaymentRecordMapperError: Error {
    case missingRequiredField
}

private extension W3sPaymentRecord.Status {
    init(rawValue: Int16, reason: String?) {
        switch rawValue {
        case 0: self = .pending
        case 1: self = .submitted
        case 2: self = .failed(reason: reason)
        case 3: self = .revoked
        case 4: self = .sent
        case 5: self = .claimed
        default: self = .failed(reason: "Unknown persisted status \(rawValue)")
        }
    }

    var rawValue: Int16 {
        switch self {
        case .pending: 0
        case .submitted: 1
        case .failed: 2
        case .revoked: 3
        case .sent: 4
        case .claimed: 5
        }
    }

    var failureReason: String? {
        guard case let .failed(reason) = self else {
            return nil
        }
        return reason
    }
}
