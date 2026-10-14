import Coinage
import CoreData
import Foundation
import Operation_iOS
import StructuredConcurrency

/// CoreData-backed ``CoinageInstallationRepositoryProtocol``: the previous installations the data store
/// lists, each with its scan cursors. Every operation runs as one block on the store's single context.
final class CoinageInstallationCoreDataRepository: CoinageInstallationRepositoryProtocol, @unchecked Sendable {
    private let databaseService: CoreDataServiceProtocol

    init(storageFacade: StorageFacadeProtocol) {
        databaseService = storageFacade.databaseService
    }

    func addPrevious(_ installations: [CoinageInstallationId]) async throws {
        let previous = Set(installations)
        guard !previous.isEmpty else { return }

        try await databaseService.performWrite { context in
            for installation in previous {
                let existing: CDCoinageInstallation? = try context.first(for: Self.predicate(for: installation))
                guard existing == nil else { continue }

                let row = try context.insertNew(CDCoinageInstallation.self)
                row.identifier = installation.hex
                row.coinScanNextIndex = 0
                row.voucherScanNextIndex = 0
                row.initialScanCompleted = false
                row.isUserConfirmedCompletion = false
            }
        }
    }

    func getPrevious() async throws -> [PreviousInstallation] {
        try await databaseService.performRead { context in
            let request = NSFetchRequest<CDCoinageInstallation>(entityName: Self.entityName)
            let byIdentifier = NSSortDescriptor(key: #keyPath(CDCoinageInstallation.identifier), ascending: true)
            request.sortDescriptors = [byIdentifier]

            return try context.fetch(request).map { row in
                try PreviousInstallation(
                    id: CoinageInstallationId(hex: Self.identifier(of: row)),
                    coinScanNextIndex: UInt64(bitPattern: row.coinScanNextIndex),
                    voucherScanNextIndex: UInt64(bitPattern: row.voucherScanNextIndex),
                    initialScanCompleted: row.initialScanCompleted,
                    isUserConfirmedCompletion: row.isUserConfirmedCompletion
                )
            }
        }
    }

    func updateCoinScanNextIndex(_ nextIndex: DerivationIndex, for installation: CoinageInstallationId) async throws {
        try await update(installation) { $0.coinScanNextIndex = Int64(bitPattern: nextIndex) }
    }

    func updateVoucherScanNextIndex(
        _ nextIndex: DerivationIndex,
        for installation: CoinageInstallationId
    ) async throws {
        try await update(installation) { $0.voucherScanNextIndex = Int64(bitPattern: nextIndex) }
    }

    func markInitialScanCompleted(_ installation: CoinageInstallationId) async throws {
        try await update(installation) { $0.initialScanCompleted = true }
    }

    func markAllUserConfirmed() async throws {
        try await databaseService.performWrite { context in
            let request = NSFetchRequest<CDCoinageInstallation>(entityName: Self.entityName)
            for row in try context.fetch(request) {
                row.isUserConfirmedCompletion = true
            }
        }
    }
}

private extension CoinageInstallationCoreDataRepository {
    static let entityName = "CDCoinageInstallation"

    static func predicate(for installation: CoinageInstallationId) -> NSPredicate {
        NSPredicate(format: "%K == %@", #keyPath(CDCoinageInstallation.identifier), installation.hex)
    }

    static func identifier(of row: CDCoinageInstallation) throws -> String {
        guard let identifier = row.identifier else {
            throw CoreDataMapperError.missingRequiredData(keyPath: #keyPath(CDCoinageInstallation.identifier))
        }
        return identifier
    }

    func update(
        _ installation: CoinageInstallationId,
        _ change: @escaping (CDCoinageInstallation) -> Void
    ) async throws {
        try await databaseService.performWrite { context in
            guard let row: CDCoinageInstallation = try context.first(for: Self.predicate(for: installation)) else {
                throw CoinageInstallationRepositoryError.unknownInstallation(installation)
            }
            change(row)
        }
    }
}

enum CoinageInstallationRepositoryError: Error {
    case unknownInstallation(CoinageInstallationId)
}
