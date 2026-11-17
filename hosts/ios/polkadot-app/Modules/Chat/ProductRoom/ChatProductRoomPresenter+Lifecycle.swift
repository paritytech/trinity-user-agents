import Foundation
import UIKit

/// The room of a product closes while the app is in the background, and opens again when the app comes back
/// with the room screen on top.
///
/// The room screen opens and closes the room when it appears and disappears. The app does not tell a screen that
/// it goes to the background, so this observer sends the same open and close for that transition.
@MainActor
final class ChatRoomBackgroundObserver {
    private let openRoom: () -> Void
    private let closeRoom: () -> Void
    private var isVisible = false
    private var isInBackground = false
    private var observers: [NSObjectProtocol] = []

    init(
        notificationCenter: NotificationCenter = .default,
        openRoom: @escaping () -> Void,
        closeRoom: @escaping () -> Void
    ) {
        self.openRoom = openRoom
        self.closeRoom = closeRoom

        observers = [
            notificationCenter.addObserver(
                forName: UIApplication.didEnterBackgroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.enterBackground() }
            },
            notificationCenter.addObserver(
                forName: UIApplication.willEnterForegroundNotification,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.enterForeground() }
            }
        ]
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    /// The room screen appeared. Its own open runs.
    func screenDidAppear() {
        isVisible = true
    }

    /// The room screen disappeared. Its own close runs.
    func screenDidDisappear() {
        isVisible = false
    }

    func enterBackground() {
        guard isVisible, !isInBackground else { return }
        isInBackground = true
        closeRoom()
    }

    func enterForeground() {
        guard isInBackground else { return }
        isInBackground = false
        if isVisible {
            openRoom()
        }
    }
}
