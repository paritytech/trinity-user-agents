import CoreData
import Foundation

/// The period an outgoing statement's current copy is funded for; all other state lives on its message and request.
extension NSManagedObjectContext {
    func outgoingStatementPeriod(for statementId: String) throws -> UInt32? {
        try outgoingStatementPeriods(for: [statementId])[statementId]
    }

    func outgoingStatementPeriods(for statementIds: [String]) throws -> [String: UInt32] {
        guard !statementIds.isEmpty else { return [:] }

        let fetchRequest = CDOutgoingStatementPublication.fetchRequest()
        fetchRequest.predicate = NSPredicate(
            format: "%K IN %@",
            #keyPath(CDOutgoingStatementPublication.statementId),
            statementIds
        )

        return try fetch(fetchRequest).reduce(into: [:]) { periods, publication in
            guard let statementId = publication.statementId else { return }
            periods[statementId] = UInt32(bitPattern: publication.period)
        }
    }

    func setOutgoingStatementPeriod(_ period: UInt32, for statementId: String) throws {
        let publication = try fetchOutgoingStatementPublication(for: statementId)
            ?? CDOutgoingStatementPublication(context: self)

        publication.statementId = statementId
        publication.period = Int32(bitPattern: period)
    }

    func deleteOutgoingStatementPublication(for statementId: String) throws {
        try fetchOutgoingStatementPublication(for: statementId).map(delete)
    }
}

private extension NSManagedObjectContext {
    func fetchOutgoingStatementPublication(for statementId: String) throws -> CDOutgoingStatementPublication? {
        let fetchRequest = CDOutgoingStatementPublication.fetchRequest()
        fetchRequest.predicate = NSPredicate(
            format: "%K == %@",
            #keyPath(CDOutgoingStatementPublication.statementId),
            statementId
        )
        fetchRequest.fetchLimit = 1

        return try fetch(fetchRequest).first
    }
}
