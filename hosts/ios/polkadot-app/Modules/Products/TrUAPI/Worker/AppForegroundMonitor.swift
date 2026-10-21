import Foundation
import os
import UIKit

/// Follows whether the app is in the foreground, so that the core holds the foreground workers
/// (VERBOSITY-WASM-PRODUCT I5).
///
/// The app is in the foreground from `willEnterForeground` to `didEnterBackground`. An app that only resigns active
/// (a system alert, Face ID, Control Center) stays in the foreground, as the room screens do (S24). iOS does not
/// suspend such an app, and a stop at that time ends the worker during a signing prompt.
final class AppForegroundMonitor: @unchecked Sendable {
    private struct State {
        var isForeground: Bool
        var handlers: [@Sendable (Bool) -> Void] = []
    }

    private let state: OSAllocatedUnfairLock<State>
    private let notificationCenter: NotificationCenter
    private var observers: [NSObjectProtocol] = []

    /// A monitor that starts with `isForeground` and then follows the notifications of `notificationCenter`.
    init(isForeground: Bool, notificationCenter: NotificationCenter = .default) {
        state = OSAllocatedUnfairLock(initialState: State(isForeground: isForeground))
        self.notificationCenter = notificationCenter

        observers = [
            notificationCenter.addObserver(
                forName: UIApplication.didEnterBackgroundNotification,
                object: nil,
                queue: nil
            ) { [weak self] _ in
                self?.update(false)
            },
            notificationCenter.addObserver(
                forName: UIApplication.willEnterForegroundNotification,
                object: nil,
                queue: nil
            ) { [weak self] _ in
                self?.update(true)
            }
        ]
    }

    /// A monitor that starts with the state of the app now. An app that iOS launches in the background starts in
    /// the background.
    @MainActor
    convenience init() {
        self.init(isForeground: UIApplication.shared.applicationState != .background)
    }

    deinit {
        observers.forEach(notificationCenter.removeObserver)
    }

    var isForeground: Bool {
        state.withLock { $0.isForeground }
    }

    /// Calls `handler` with the new state at each change, on the thread of the notification.
    func onChange(_ handler: @escaping @Sendable (Bool) -> Void) {
        state.withLock { $0.handlers.append(handler) }
    }

    private func update(_ isForeground: Bool) {
        let handlers: [@Sendable (Bool) -> Void]? = state.withLock { state in
            guard state.isForeground != isForeground else { return nil }

            state.isForeground = isForeground
            return state.handlers
        }
        handlers?.forEach { $0(isForeground) }
    }
}
