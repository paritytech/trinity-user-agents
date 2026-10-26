import Foundation
import Keystore_iOS
import KeyDerivation
import SubstrateSdk

extension AppSharedInitializer {
    /// Shared by the root screen and a headless launch, so migrations run once per process.
    static let shared = AppSharedInitializer(
        dependencies: AppSharedInitializerDependencies(
            migrator: makeLaunchMigrator(),
            chainRegistryClosure: { ChainRegistryFacade.sharedRegistry },
            remoteConfigManager: FirebaseFacade.shared,
            chainRegistryConfigurator: FirebaseFacade.shared
        ),
        logger: Logger.shared
    )
}

private extension AppSharedInitializer {
    /// Local launch steps in order: erase a cross-device backup restore before any store is opened, then
    /// migrate the schemas.
    static func makeLaunchMigrator() -> Migrating {
        let restoredBackupGuard = RestoredBackupGuard(
            keyIdStore: InstallationKeyIdStore(),
            entropyManager: RootEntropyManager.shared,
            eraser: LocalStateEraser(logger: Logger.shared),
            logger: Logger.shared
        )

        let userStorageMigrator = UserStorageMigrator(
            storeURL: UserStorageParams.storageURL,
            modelDirectory: UserStorageParams.modelDirectory,
            model: UserStorageParams.modelVersion,
            fileManager: FileManager.default
        )

        let substrateStorageMigrator = SubstrateStorageMigrator(
            storeURL: SubstrateStorageParams.storageURL,
            modelDirectory: SubstrateStorageParams.modelDirectory,
            model: SubstrateStorageParams.modelVersion,
            fileManager: FileManager.default
        )

        return SerialMigrator(migrations: [restoredBackupGuard, userStorageMigrator, substrateStorageMigrator])
    }
}
