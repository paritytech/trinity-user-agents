import Foundation
import ChainRegistry

/// Main actor: migrations must finish before any caller opens a store, and the config fetch is not thread-safe.
@MainActor
protocol AppSharedInitializing: AnyObject {
    func start()

    func refetchConfig()
}

struct AppSharedInitializerDependencies {
    let migrator: Migrating
    let chainRegistryClosure: ChainRegistryLazyClosure
    let remoteConfigManager: RemoteConfigManaging
    let chainRegistryConfigurator: ChainRegistryConfiguring
}

@MainActor
final class AppSharedInitializer {
    private let dependencies: AppSharedInitializerDependencies
    private let logger: LoggerProtocol
    private var didStart = false

    init(dependencies: AppSharedInitializerDependencies, logger: LoggerProtocol) {
        self.dependencies = dependencies
        self.logger = logger
    }
}

extension AppSharedInitializer: AppSharedInitializing {
    func start() {
        guard !didStart else { return }
        didStart = true

        runMigrations()
        dependencies.chainRegistryConfigurator.set(chainRegistry: dependencies.chainRegistryClosure())
        refetchConfig()
    }

    func refetchConfig() {
        dependencies.remoteConfigManager.fetchRemoteConfigValues()
    }
}

private extension AppSharedInitializer {
    func runMigrations() {
        do {
            try dependencies.migrator.migrate()
        } catch {
            logger.error("Migration failed: \(error.localizedDescription)")
        }
    }
}
