import Foundation

extension RootGate {
    struct Theme: DecisionGate {
        private let storage: ThemeSelectionStoring

        init(storage: ThemeSelectionStoring = ThemeSelectionStorage()) {
            self.storage = storage
        }

        func evaluate() -> RootDestination? {
            storage.hasSelectedTheme ? nil : .selectTheme
        }
    }
}
