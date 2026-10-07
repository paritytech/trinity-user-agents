import DesignSystem
import Foundation
import UIKit

@MainActor
final class MockThemeManager: ThemeManagerProtocol {
    private(set) var theme: Theme
    private(set) var mode: ThemeMode

    private var observers: [UUID: AsyncStream<Theme>.Continuation] = [:]

    nonisolated init(selection: ThemeSelection = ThemesRegistry.default) {
        mode = .app(selection)
        theme = ThemesRegistry.makeTheme(selection)
    }

    func observeTheme() -> AsyncStream<Theme> {
        AsyncStream { continuation in
            let id = UUID()
            observers[id] = continuation
            continuation.yield(theme)
            continuation.onTermination = { [weak self] _ in
                Task { @MainActor in
                    self?.observers[id] = nil
                }
            }
        }
    }

    func select(_ mode: ThemeMode) {
        self.mode = mode

        switch mode {
        case let .app(selection):
            theme = ThemesRegistry.makeTheme(selection)
        }

        observers.values.forEach { $0.yield(theme) }
    }

    func setup(scene _: UIWindowScene) {}
}
