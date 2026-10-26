import Foundation
import Keystore_iOS
import KeyDerivation

extension BackgroundTaskBootstrapper {
    @MainActor
    static func makeDefault() -> BackgroundTaskBootstrapper {
        BackgroundTaskBootstrapper(
            dependencies: BackgroundTaskBootstrapDependencies(
                initializer: AppSharedInitializer.shared,
                chainRegistryClosure: { ChainRegistryFacade.sharedRegistry },
                remoteConfigManager: FirebaseFacade.shared,
                tldProvider: DotNsTldProviderFacade.shared,
                appliedConfigReader: { AppConfigProvider.shared.getRemoteConfig() },
                destinationResolver: makeDestinationResolver()
            ),
            clock: ContinuousClock(),
            logger: Logger.shared
        )
    }
}

private extension BackgroundTaskBootstrapper {
    /// Without the theme gate, which only the UI can complete.
    static func makeDestinationResolver() -> any DecisionResolver<RootDestination> {
        SequentialDecisionResolver<RootDestination>(
            gates: [
                RootGate.Wallet(entropyManager: RootEntropyManager.shared, backupHelper: MnemonicBackupHelper()),
                RootGate.Username(usernameStorage: UsernameStorage())
            ],
            fallback: .dashboard
        )
    }
}
