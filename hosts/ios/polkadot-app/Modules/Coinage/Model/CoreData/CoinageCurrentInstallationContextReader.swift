import Coinage
import CoreData
import Foundation
import os

/// Reads the `CDCurrentInstallation` row on the context of the row being mapped.
final class CoinageCurrentInstallationContextReader: @unchecked Sendable {
    private let cached = OSAllocatedUnfairLock<CoinageInstallationId?>(initialState: nil)

    func current(in context: NSManagedObjectContext) throws -> CoinageInstallationId? {
        if let known = cached.withLock({ $0 }) { return known }

        let request = NSFetchRequest<CDCurrentInstallation>(entityName: Self.entityName)
        request.fetchLimit = 1
        guard let row = try context.fetch(request).first else { return nil }
        guard let identifier = row.identifier else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCurrentInstallation.identifier))
        }

        // The row never changes once created; a missing row is not cached because it appears later.
        let installation = try CoinageInstallationId(hex: identifier)
        cached.withLock { $0 = installation }
        return installation
    }
}

extension CoinageCurrentInstallationContextReader {
    func isRecovered(_ index: CoinageKeyIndex, of entity: NSManagedObject) throws -> Bool {
        guard let context = entity.managedObjectContext else {
            throw CoreDataMapperError.missingRequiredData(keyPath: "managedObjectContext")
        }
        guard let current = try current(in: context) else { return false }
        return index.installation != current
    }
}

private extension CoinageCurrentInstallationContextReader {
    static let entityName = "CDCurrentInstallation"
}
