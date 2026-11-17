import Foundation
import FoundationExt

enum DashboardServices {
    static let flowStateProvider: SPAFlowStateProviding = SPAFlowStateProvider()

    @MainActor static let manager = DashboardServicesManager<ServiceCoordinator>(
        background: DashboardServicesBackground(
            bootstrapper: BackgroundTaskBootstrapper.makeDefault(),
            deadline: .seconds(20)
        ),
        coordinatorFactory: ServiceCoordinatorFactory(flowStateProvider: flowStateProvider),
        connectionRetention: ChainRegistryConnectionRetention(),
        activity: ApplicationActivity(),
        logger: Logger.shared
    )
}

struct ServiceCoordinatorFactory: DashboardServicesCoordinatorMaking {
    let flowStateProvider: SPAFlowStateProviding

    @MainActor
    func makeCoordinator() -> ServiceCoordinator? {
        ServiceCoordinator.createDefault(spaFlowState: flowStateProvider.flowState())
    }
}
