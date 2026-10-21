import Foundation
import UIKit

/// The live parts of the footer of a product room: the button that stops "Read aloud" (Chat Extension v2,
/// 02-DESIGN §6.6).
///
/// The room screen keeps one model for a room and changes it in place. The footer view observes it, so a change
/// does not rebuild the input and the text that the user types stays.
@MainActor
public final class ChatRoomAccessoryModel {
    /// The title of the button that stops "Read aloud". Nil hides the button: the room speaks nothing.
    public var readAloudStopTitle: String? {
        didSet { if readAloudStopTitle != oldValue { notify() } }
    }

    /// The user pressed the button that stops "Read aloud".
    public var onStopReadAloud: (() -> Void)?

    private var observers: [UUID: () -> Void] = [:]

    public init() {}

    /// Calls `observer` after each change, until the token goes away.
    public func observe(_ observer: @escaping () -> Void) -> ChatRoomAccessoryObservation {
        let token = UUID()
        observers[token] = observer
        return ChatRoomAccessoryObservation { [weak self] in
            MainActor.assumeIsolated { self?.observers[token] = nil }
        }
    }

    private func notify() {
        observers.values.forEach { $0() }
    }
}

/// Removes an observer of a ``ChatRoomAccessoryModel`` when it goes away.
public final class ChatRoomAccessoryObservation {
    private let cancel: () -> Void

    init(cancel: @escaping () -> Void) {
        self.cancel = cancel
    }

    deinit {
        cancel()
    }
}
