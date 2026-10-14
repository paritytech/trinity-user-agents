import BigInt
import Coinage
import CoreData
import Operation_iOS

/// Maps between ``ExternalPayment`` domain models and `CDExternalPayment` CoreData entities.
final class ExternalPaymentMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = ExternalPayment
    typealias CoreDataEntity = CDExternalPayment

    var entityIdentifierFieldName: String { #keyPath(CDExternalPayment.identifier) }

    func transform(entity: CDExternalPayment) throws -> ExternalPayment {
        guard let productId = entity.productId,
              let paymentId = entity.paymentId,
              let amountString = entity.amountInPlanks,
              let destination = entity.destination,
              let createdAt = entity.createdAt,
              let updatedAt = entity.updatedAt
        else {
            throw ExternalPaymentMapperError.missingRequiredField
        }

        let amount = BigUInt(amountString) ?? 0
        let stage = ExternalPayment.Stage(rawValue: Int(entity.stage)) ?? .plan
        let settled = entity.settledInPlanks.flatMap { BigUInt($0) } ?? 0
        let surplus = entity.surplusInPlanks.flatMap { BigUInt($0) } ?? 0

        let plannedVoucherIndices = try Self.decodeVoucherIndices(entity.plannedVoucherIndices)

        return ExternalPayment(
            productId: productId,
            paymentId: paymentId,
            amountInPlanks: amount,
            destination: destination,
            settledInPlanks: settled,
            stage: stage,
            plannedVoucherIndices: plannedVoucherIndices,
            surplusInPlanks: surplus,
            failureReason: entity.failureReason,
            createdAt: createdAt,
            updatedAt: updatedAt
        )
    }

    func populate(
        entity: CDExternalPayment,
        from model: ExternalPayment,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.productId = model.productId
        entity.paymentId = model.paymentId
        entity.amountInPlanks = String(model.amountInPlanks)
        entity.destination = model.destination
        entity.settledInPlanks = String(model.settledInPlanks)
        entity.stage = Int16(model.stage.rawValue)
        entity.plannedVoucherIndices = Self.encodeVoucherIndices(model.plannedVoucherIndices)
        entity.surplusInPlanks = String(model.surplusInPlanks)
        entity.failureReason = model.failureReason
        entity.createdAt = model.createdAt
        entity.updatedAt = model.updatedAt
    }

    /// Comma-separated key indices in their string form; nil for none.
    static func encodeVoucherIndices(_ indices: [CoinageKeyIndex]) -> String? {
        indices.isEmpty ? nil : indices.map { $0.toString() }.joined(separator: ",")
    }

    static func decodeVoucherIndices(_ encoded: String?) throws -> [CoinageKeyIndex] {
        try encoded?.split(separator: ",").map { try CoinageKeyIndex.fromString(String($0)) } ?? []
    }
}

/// Partial mapper that updates only mutable fields on an existing `CDExternalPayment`.
///
/// Use for stage transitions — avoids re-writing immutable fields like origin, destination, amount.
final class ExternalPaymentStageMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case noExistingEntity
    }

    typealias DataProviderModel = ExternalPayment
    typealias CoreDataEntity = CDExternalPayment

    var entityIdentifierFieldName: String { #keyPath(CDExternalPayment.identifier) }

    func transform(entity: CDExternalPayment) throws -> ExternalPayment {
        try ExternalPaymentMapper().transform(entity: entity)
    }

    func populate(
        entity: CDExternalPayment,
        from model: ExternalPayment,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.noExistingEntity
        }
        entity.stage = Int16(model.stage.rawValue)
        entity.settledInPlanks = String(model.settledInPlanks)
        entity.plannedVoucherIndices = ExternalPaymentMapper.encodeVoucherIndices(model.plannedVoucherIndices)
        entity.surplusInPlanks = String(model.surplusInPlanks)
        entity.failureReason = model.failureReason
        entity.updatedAt = model.updatedAt
    }
}

private enum ExternalPaymentMapperError: Error {
    case missingRequiredField
}
