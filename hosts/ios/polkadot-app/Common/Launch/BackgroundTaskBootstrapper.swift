import Foundation
import ChainRegistry
import Products
import StructuredConcurrency

enum BackgroundTaskReadiness: Equatable {
    case dashboard
    case notOnboarded
    case configUnavailable
    case tldUnavailable
    case timedOut
}

protocol BackgroundTaskBootstrapping: Sendable {
    func waitWhenReady(deadline: Duration) async -> BackgroundTaskReadiness
}

struct BackgroundTaskBootstrapDependencies {
    let initializer: AppSharedInitializing
    let chainRegistryClosure: ChainRegistryLazyClosure
    let remoteConfigManager: RemoteConfigManaging
    let tldProvider: DotNsTldProviding
    let appliedConfigReader: () -> RemoteAppConfig?
    let destinationResolver: any DecisionResolver<RootDestination>
}

final class BackgroundTaskBootstrapper {
    private static let durabilityChainIds: Set<ChainModel.Id> = [
        AppConfig.Chains.chatChain,
        AppConfig.Assets.mainAsset.chainId
    ]

    private let dependencies: BackgroundTaskBootstrapDependencies
    private let clock: any Clock<Duration>
    private let logger: LoggerProtocol

    init(dependencies: BackgroundTaskBootstrapDependencies, clock: any Clock<Duration>, logger: LoggerProtocol) {
        self.dependencies = dependencies
        self.clock = clock
        self.logger = logger
    }
}

extension BackgroundTaskBootstrapper: BackgroundTaskBootstrapping, @unchecked Sendable {
    func waitWhenReady(deadline: Duration) async -> BackgroundTaskReadiness {
        await dependencies.initializer.start()

        do {
            try await withTimeout(deadline, clock: clock) { [self] in
                _ = try await dependencies.remoteConfigManager.asyncWaitRemoteConfig()
                await dependencies.chainRegistryClosure().asyncWaitChainsSetup(for: Self.durabilityChainIds)
            }

            return evaluateReadiness()
        } catch {
            logger.warning("Background task readiness wait ended: \(error)")
            return dependencies.appliedConfigReader()?.isValid == true ? .timedOut : .configUnavailable
        }
    }
}

private extension BackgroundTaskBootstrapper {
    func evaluateReadiness() -> BackgroundTaskReadiness {
        guard dependencies.appliedConfigReader()?.isValid == true else {
            return .configUnavailable
        }

        // Not resolved over the network here: the root screen does that on first launch.
        guard dependencies.tldProvider.currentTld() != nil else {
            return .tldUnavailable
        }

        guard (try? dependencies.destinationResolver.resolve()) == .dashboard else {
            return .notOnboarded
        }

        return .dashboard
    }
}
