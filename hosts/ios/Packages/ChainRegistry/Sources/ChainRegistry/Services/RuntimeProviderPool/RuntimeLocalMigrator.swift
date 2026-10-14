import Foundation

public protocol RuntimeLocalMigrating {
    var version: UInt32 { get }
}

public extension RuntimeLocalMigrating {
    func needsMigration(for runtimeItem: RuntimeMetadataItem) -> Bool {
        runtimeItem.localMigratorVersion < version
    }
}

public enum RuntimeLocalVersion {
    public static let latest: UInt32 = 2
}

public struct RuntimeLocalMigrator: RuntimeLocalMigrating {
    public let version: UInt32
}

public extension RuntimeLocalMigrator {
    static func createLatest() -> RuntimeLocalMigrator {
        RuntimeLocalMigrator(version: RuntimeLocalVersion.latest)
    }
}
