import ChainRegistry
import Foundation
import os

enum DashboardServicesDemand: Hashable {
    case interactive
    case durability
}

final class DashboardServicesLease<Coordinator: DashboardServicesCoordinating>: @unchecked Sendable {
    let coordinator: Coordinator
    let demand: DashboardServicesDemand
    let generation: UInt64

    private let connectionToken: (any ConnectionTokenProtocol)?
    private let onRelease: @MainActor (DashboardServicesLease) -> Void
    private let isReleased = OSAllocatedUnfairLock(initialState: false)

    init(
        coordinator: Coordinator,
        demand: DashboardServicesDemand,
        generation: UInt64,
        connectionToken: (any ConnectionTokenProtocol)?,
        onRelease: @escaping @MainActor (DashboardServicesLease) -> Void
    ) {
        self.coordinator = coordinator
        self.demand = demand
        self.generation = generation
        self.connectionToken = connectionToken
        self.onRelease = onRelease
    }

    func release() {
        let wasReleased = isReleased.withLock { released in
            defer { released = true }
            return released
        }

        guard !wasReleased else { return }

        connectionToken?.release()
        Task { @MainActor [onRelease] in onRelease(self) }
    }
}
