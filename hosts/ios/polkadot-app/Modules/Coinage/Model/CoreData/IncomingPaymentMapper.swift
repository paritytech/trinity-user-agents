import BigInt
import Coinage
import CoreData
import Foundation
import Operation_iOS

/// Maps between ``IncomingPayment`` domain models and `CDIncomingPayment` CoreData entities.
///
/// The record holds no secret material (that lives in the Keychain via `IncomingPaymentSecretStoring`)
/// and no live status — only the identity, amount, window start, and, once settled, the terminal
/// verdict (`outcomeTag` + `actualClaimed`).
final class IncomingPaymentMapper: CoreDataMapperProtocol {
    typealias DataProviderModel = IncomingPayment
    typealias CoreDataEntity = CDIncomingPayment

    var entityIdentifierFieldName: String { #keyPath(CDIncomingPayment.identifier) }

    func transform(entity: CDIncomingPayment) throws -> IncomingPayment {
        guard let paymentId = entity.paymentId,
              let productId = entity.productId,
              let amountString = entity.amount,
              let amount = BigUInt(amountString),
              let createdAt = entity.createdAt
        else {
            throw IncomingPaymentMapperError.missingRequiredField
        }

        return try IncomingPayment(
            paymentId: paymentId,
            productId: productId,
            amount: amount,
            createdAt: createdAt,
            outcome: IncomingPaymentOutcomeSerialization.outcome(
                tag: entity.outcomeTag,
                actualClaimed: entity.actualClaimed
            )
        )
    }

    func populate(
        entity: CDIncomingPayment,
        from model: IncomingPayment,
        using _: NSManagedObjectContext
    ) throws {
        entity.identifier = model.groupId
        entity.paymentId = model.paymentId
        entity.productId = model.productId
        entity.amount = String(model.amount)
        entity.createdAt = model.createdAt

        let serialized = IncomingPaymentOutcomeSerialization.columns(for: model.outcome)
        entity.outcomeTag = serialized.tag
        entity.actualClaimed = serialized.actualClaimed
    }
}

/// Write-only mapper that writes only the terminal verdict onto an existing record — never reads a
/// payment back, so settling does not fetch-modify-save the whole record (mirrors `CoinPresenceMapper`).
final class IncomingPaymentOutcomeMapper: CoreDataMapperProtocol {
    enum MappingError: Error {
        case missingPayment
    }

    typealias DataProviderModel = IncomingPaymentOutcomeUpdate
    typealias CoreDataEntity = CDIncomingPayment

    var entityIdentifierFieldName: String { #keyPath(CDIncomingPayment.identifier) }

    func transform(entity _: CDIncomingPayment) throws -> IncomingPaymentOutcomeUpdate {
        throw CoreDataMapperError.unsupported
    }

    func populate(
        entity: CDIncomingPayment,
        from model: IncomingPaymentOutcomeUpdate,
        using _: NSManagedObjectContext
    ) throws {
        guard entity.identifier != nil else {
            throw MappingError.missingPayment
        }
        let serialized = IncomingPaymentOutcomeSerialization.columns(for: model.outcome)
        entity.outcomeTag = serialized.tag
        entity.actualClaimed = serialized.actualClaimed
    }
}

// MARK: - Outcome ↔ columns

enum IncomingPaymentOutcomeSerialization {
    private static let claimed = "claimed"
    private static let claimedPartially = "claimedPartially"
    private static let notClaimed = "notClaimed"

    static func columns(for outcome: IncomingPaymentTerminalOutcome?) -> (tag: String?, actualClaimed: String?) {
        switch outcome {
        case .none: (nil, nil)
        case .claimed: (claimed, nil)
        case let .claimedPartially(actual): (claimedPartially, String(actual))
        case .notClaimed: (notClaimed, nil)
        }
    }

    static func outcome(tag: String?, actualClaimed: String?) throws -> IncomingPaymentTerminalOutcome? {
        guard let tag else { return nil }
        switch tag {
        case claimed:
            return .claimed
        case claimedPartially:
            guard let actualClaimed, let value = BigUInt(actualClaimed) else {
                throw IncomingPaymentMapperError.missingRequiredField
            }
            return .claimedPartially(actualClaimed: value)
        case notClaimed:
            return .notClaimed
        default:
            throw IncomingPaymentMapperError.missingRequiredField
        }
    }
}

private enum IncomingPaymentMapperError: Error {
    case missingRequiredField
}
