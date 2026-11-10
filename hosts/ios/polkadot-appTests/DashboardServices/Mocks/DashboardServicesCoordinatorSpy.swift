import Foundation

@testable import polkadot_app

@MainActor
final class DashboardServicesCoordinatorSpy: DashboardServicesCoordinating {
    private(set) var startDurabilityCallCount = 0
    private(set) var startInteractiveCallCount = 0
    private(set) var stopInteractiveCallCount = 0
    private(set) var stopAllCallCount = 0

    private let durabilityStarts: Bool

    init(durabilityStarts: Bool = true) {
        self.durabilityStarts = durabilityStarts
    }

    func startDurability() async -> Bool {
        startDurabilityCallCount += 1
        return durabilityStarts
    }

    func startInteractive() {
        startInteractiveCallCount += 1
    }

    func stopInteractive() {
        stopInteractiveCallCount += 1
    }

    func stopAll() {
        stopAllCallCount += 1
    }
}
