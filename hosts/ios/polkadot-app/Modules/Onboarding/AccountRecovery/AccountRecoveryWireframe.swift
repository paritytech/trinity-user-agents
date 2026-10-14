import Foundation

@MainActor
final class AccountRecoveryWireframe: AccountRecoveryWireframeProtocol {
    private let observer: RootStateObserving
    init(observer: RootStateObserving) {
        self.observer = observer
    }

    func didDecideBroken() {
        observer.didDecideBroken()
    }

    func didRestoreWallets() {
        observer.didRestoreWallets()
    }
}
