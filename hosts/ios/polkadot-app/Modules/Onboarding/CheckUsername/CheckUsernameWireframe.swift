import Foundation

final class CheckUsernameWireframe: CheckUsernameWireframeProtocol {
    let observer: RootStateObserving

    init(observer: RootStateObserving) {
        self.observer = observer
    }

    func showMainScreen() {
        MainActor.assumeIsolated {
            observer.didClaimUsername()
        }
    }

    func showClaimUsername() {
        MainActor.assumeIsolated {
            observer.didDecideClaim()
        }
    }
}
