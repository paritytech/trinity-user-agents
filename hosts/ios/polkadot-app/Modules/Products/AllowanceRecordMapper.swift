import CoreData
import Foundation
import Operation_iOS
import Individuality

final class AllowanceRecordMapper: CoreDataMapperProtocol {
    var entityIdentifierFieldName: String { #keyPath(CDAllowanceRecord.identifier) }

    typealias DataProviderModel = AllowanceRecord
    typealias CoreDataEntity = CDAllowanceRecord

    func transform(entity: CDAllowanceRecord) throws -> AllowanceRecord {
        guard let accountId = entity.accountId else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDAllowanceRecord.accountId))
        }
        guard let allocatedAt = entity.allocatedAt else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDAllowanceRecord.allocatedAt))
        }
        let kind = entity.kind == -1 ? nil : AllowanceRecord.Kind(persistenceCode: entity.kind)
        // An unknown code comes from a newer build; product is never released by automatic cleanup.
        let domain = AllowanceRecord.Domain(persistenceCode: entity.domain) ?? .product
        let priority = AllowanceRecord.Priority(rawValue: Int(entity.priorityLevel)) ?? .normal
        let latestRenewedPeriod = entity.latestRenewedPeriod == -1 ? nil : UInt32(entity.latestRenewedPeriod)
        return AllowanceRecord(
            accountId: accountId,
            allocatedAt: allocatedAt,
            kind: kind,
            domain: domain,
            priority: priority,
            latestRenewedPeriod: latestRenewedPeriod
        )
    }

    func populate(
        entity: CDAllowanceRecord,
        from model: AllowanceRecord,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.identifier
        entity.accountId = model.accountId
        entity.allocatedAt = model.allocatedAt
        entity.kind = model.kind?.persistenceCode ?? -1
        entity.domain = model.domain.persistenceCode
        entity.priorityLevel = Int16(model.priority.rawValue)
        entity.latestRenewedPeriod = model.latestRenewedPeriod.map { Int64($0) } ?? -1
    }
}
