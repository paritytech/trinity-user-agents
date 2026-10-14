import Foundation

@testable import polkadot_app

final class MockMigrator: Migrating {
    private(set) var migrateCallCount = 0

    func migrate() throws {
        migrateCallCount += 1
    }
}
