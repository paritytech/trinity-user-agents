import CoreData
import Foundation
import Operation_iOS
import TrUAPIHost

/// The host's funding history, one row per ended session, keyed by its id so
/// writing a session again updates its row.
final class CoreDataFundingHistoryStore: FundingHistoryStoring, @unchecked Sendable {
    private let repository: AnyDataProviderRepository<FundingRecord>

    init(storageFacade: StorageFacadeProtocol = UserDataStorageFacade.shared) {
        repository = AnyDataProviderRepository(
            storageFacade.createRepository(mapper: AnyCoreDataMapper(FundingRecordMapper()))
        )
    }

    func records() async throws -> [FundingRecord] {
        try await repository.fetchAllOperation(with: RepositoryFetchOptions()).asyncExecute()
    }

    func save(_ record: FundingRecord) async throws {
        try await repository.saveOperation({ [record] }, { [] }).asyncExecute()
    }
}

extension FundingRecord: Operation_iOS.Identifiable {
    var identifier: String { intent }
}

final class FundingRecordMapper {
    var entityIdentifierFieldName: String {
        #keyPath(CoreDataEntity.identifier)
    }

    typealias DataProviderModel = FundingRecord
    typealias CoreDataEntity = CDFundingRecord
}

extension FundingRecordMapper: CoreDataMapperProtocol {
    func transform(entity: CDFundingRecord) throws -> FundingRecord {
        guard
            let identifier = entity.identifier,
            let direction = entity.direction.flatMap(FundingDirection.init(storageValue:)),
            let outcome = entity.outcome.flatMap({ FundingRecord.Outcome(storageValue: $0, code: entity.failureCode) }),
            let openedAt = entity.openedAt,
            let settledAt = entity.settledAt
        else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDFundingRecord.identifier))
        }

        return FundingRecord(
            intent: identifier,
            direction: direction,
            rail: entity.rail.flatMap(FundingRail.init(storageValue:)),
            asset: entity.asset,
            providerId: entity.providerId,
            requestedAmount: entity.requestedAmount,
            settledAmount: entity.settledAmount,
            outcome: outcome,
            payout: entity.payout.flatMap { FundingRecord.Payout(storageValue: $0, reason: entity.payoutReason) },
            transactionId: entity.transactionId,
            reference: entity.reference,
            openedAt: openedAt,
            settledAt: settledAt
        )
    }

    func populate(entity: CDFundingRecord, from model: FundingRecord, using _: NSManagedObjectContext) throws {
        entity.identifier = model.intent
        entity.direction = model.direction.storageValue
        entity.rail = model.rail?.storageValue
        entity.asset = model.asset
        entity.providerId = model.providerId
        entity.requestedAmount = model.requestedAmount
        entity.settledAmount = model.settledAmount
        entity.outcome = model.outcome.storageValue
        entity.failureCode = model.outcome.failureCode
        entity.payout = model.payout?.storageValue
        entity.payoutReason = model.payout?.reason
        entity.transactionId = model.transactionId
        entity.reference = model.reference
        entity.openedAt = model.openedAt
        entity.settledAt = model.settledAt
    }
}

private extension FundingDirection {
    init?(storageValue: String) {
        switch storageValue {
        case "in": self = .in
        case "out": self = .out
        default: return nil
        }
    }

    var storageValue: String {
        switch self {
        case .in: "in"
        case .out: "out"
        }
    }
}

private extension FundingRail {
    init?(storageValue: String) {
        switch storageValue {
        case "card": self = .card
        case "bank": self = .bank
        case "crypto": self = .crypto
        default: return nil
        }
    }

    var storageValue: String {
        switch self {
        case .card: "card"
        case .bank: "bank"
        case .crypto: "crypto"
        }
    }
}

private extension FundingRecord.Outcome {
    init?(storageValue: String, code: String?) {
        switch storageValue {
        case "delivered": self = .delivered
        case "released": self = .released
        case "refunded": self = .refunded
        case "failed": self = .failed(code: code ?? "")
        default: return nil
        }
    }

    var storageValue: String {
        switch self {
        case .delivered: "delivered"
        case .released: "released"
        case .refunded: "refunded"
        case .failed: "failed"
        }
    }

    var failureCode: String? {
        guard case let .failed(code) = self else { return nil }
        return code
    }
}

private extension FundingRecord.Payout {
    init?(storageValue: String, reason: String?) {
        switch storageValue {
        case "paidOut": self = .paidOut
        case "failed": self = .failed(reason: reason ?? "")
        default: return nil
        }
    }

    var storageValue: String {
        switch self {
        case .paidOut: "paidOut"
        case .failed: "failed"
        }
    }

    var reason: String? {
        guard case let .failed(reason) = self else { return nil }
        return reason
    }
}
