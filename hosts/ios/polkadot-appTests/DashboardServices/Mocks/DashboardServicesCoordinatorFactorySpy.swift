import Foundation

@testable import polkadot_app

@MainActor
final class DashboardServicesCoordinatorFactorySpy: DashboardServicesCoordinatorMaking {
    private(set) var made: [DashboardServicesCoordinatorSpy] = []

    private let durabilityStarts: Bool

    init(durabilityStarts: Bool = true) {
        self.durabilityStarts = durabilityStarts
    }

    func makeCoordinator() -> DashboardServicesCoordinatorSpy? {
        let coordinator = DashboardServicesCoordinatorSpy(durabilityStarts: durabilityStarts)
        made.append(coordinator)
        return coordinator
    }
}
