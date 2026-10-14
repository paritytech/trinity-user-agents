import Coinage
import CoreData
import Foundation
import Operation_iOS

/// CoreData-backed ``CoinageCurrentInstallationRepositoryProtocol``: the one `CDCurrentInstallation`
/// row. The read and the insert run as one block on the store's single context, so concurrent first
/// callers all get the id the block created.
final class CoinageCurrentInstallationCoreDataRepository: CoinageCurrentInstallationRepositoryProtocol,
    @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }

    func getOrCreateCurrent(
        newInstallation: @escaping @Sendable () throws -> CoinageInstallationId
    ) async throws -> CoinageInstallationId {
        try await databaseService.performWrite { context in
            let request = NSFetchRequest<CDCurrentInstallation>(entityName: Self.entityName)
            request.fetchLimit = 1

            if let row = try context.fetch(request).first {
                return try CoinageInstallationId(hex: Self.identifier(of: row))
            }

            let created = try newInstallation()
            let row = try context.insertNew(CDCurrentInstallation.self)
            row.identifier = created.hex
            return created
        }
    }
}

private extension CoinageCurrentInstallationCoreDataRepository {
    static let entityName = "CDCurrentInstallation"

    static func identifier(of row: CDCurrentInstallation) throws -> String {
        guard let identifier = row.identifier else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCurrentInstallation.identifier))
        }
        return identifier
    }
}
