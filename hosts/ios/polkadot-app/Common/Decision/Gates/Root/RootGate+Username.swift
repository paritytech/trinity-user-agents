import Foundation

extension RootGate {
    struct Username: DecisionGate {
        private let usernameStorage: UsernameStoring

        init(usernameStorage: UsernameStoring) {
            self.usernameStorage = usernameStorage
        }

        func evaluate() -> RootDestination? {
            usernameStorage.hasUsername ? nil : .usernameCheck
        }
    }
}
