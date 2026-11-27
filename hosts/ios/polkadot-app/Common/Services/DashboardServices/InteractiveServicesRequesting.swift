import Foundation

protocol InteractiveServicesRequesting: Sendable {
    func requestInteractiveStart()
}

/// PushKit calls back off the main thread, while the manager is bound to the main actor.
struct DashboardInteractiveServicesRequester: InteractiveServicesRequesting {
    func requestInteractiveStart() {
        Task { @MainActor in
            DashboardServices.manager.requestInteractiveStart()
        }
    }
}
