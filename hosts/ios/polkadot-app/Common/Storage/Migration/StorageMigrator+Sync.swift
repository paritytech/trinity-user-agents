import Foundation

extension UserStorageMigrator: Migrating {
    func migrate() throws {
        guard requiresMigration() else {
            return
        }

        performMigration()

        (Logger.shared as LoggerProtocol).info("User storage migration was completed")
    }
}

extension SubstrateStorageMigrator: Migrating {
    func migrate() throws {
        guard requiresMigration() else {
            return
        }

        performMigration()

        (Logger.shared as LoggerProtocol).info("Substrate storage migration was completed")
    }
}
