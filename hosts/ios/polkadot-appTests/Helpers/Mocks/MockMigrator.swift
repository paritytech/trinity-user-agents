import Foundation

@testable import polkadot_app

final class MockMigrator: Migrating {
    private(set) var migrateCallCount = 0
    private(set) var migratedOnMainThread = false

    func migrate() throws {
        migrateCallCount += 1
        migratedOnMainThread = Thread.isMainThread
    }
}
