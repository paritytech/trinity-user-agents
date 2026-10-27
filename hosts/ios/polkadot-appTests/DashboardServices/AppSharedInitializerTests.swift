import Foundation
import Testing

@testable import polkadot_app

@MainActor
struct AppSharedInitializerTests {
    @Test func launchStepsRunOnceAcrossStarts() {
        let migrator = MockMigrator()
        let remoteConfigManager = MockRemoteConfigManager()
        let initializer = makeInitializer(migrator: migrator, remoteConfigManager: remoteConfigManager)

        initializer.start()
        initializer.start()

        #expect(migrator.migrateCallCount == 1)
        #expect(remoteConfigManager.fetchCallCount == 1)
    }

    @Test func refetchConfigSkipsMigrations() {
        let migrator = MockMigrator()
        let remoteConfigManager = MockRemoteConfigManager()
        let initializer = makeInitializer(migrator: migrator, remoteConfigManager: remoteConfigManager)

        initializer.refetchConfig()

        #expect(migrator.migrateCallCount == 0)
        #expect(remoteConfigManager.fetchCallCount == 1)
    }
}

private extension AppSharedInitializerTests {
    func makeInitializer(
        migrator: MockMigrator,
        remoteConfigManager: MockRemoteConfigManager
    ) -> AppSharedInitializer {
        let chainRegistry = MockChainRegistry()

        return AppSharedInitializer(
            dependencies: AppSharedInitializerDependencies(
                migrator: migrator,
                chainRegistryClosure: { chainRegistry },
                remoteConfigManager: remoteConfigManager,
                chainRegistryConfigurator: MockChainRegistryConfigurator()
            ),
            logger: StubLogger()
        )
    }
}
