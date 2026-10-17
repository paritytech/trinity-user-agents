import Foundation
import UIKit

/// The live parts of the footer of a product room: the activity line, the quick replies and the slash commands
/// (Chat Extension v2, 02-DESIGN §6.6).
///
/// The room screen keeps one model for a room and changes it in place. The footer view observes it, so a change
/// does not rebuild the input and the text that the user types stays.
@MainActor
public final class ChatRoomAccessoryModel {
    public struct Suggestion: Hashable {
        public let suggestionId: String
        public let text: String

        public init(suggestionId: String, text: String) {
            self.suggestionId = suggestionId
            self.text = text
        }
    }

    public struct Command: Hashable {
        /// The name without the slash.
        public let name: String
        public let description: String
        public let argumentHint: String?

        public init(name: String, description: String, argumentHint: String?) {
            self.name = name
            self.description = description
            self.argumentHint = argumentHint
        }
    }

    /// The line above the input, for example "Bot is typing". Nil hides it.
    public var activityText: String? {
        didSet { if activityText != oldValue { notify() } }
    }

    /// The icon of the participant that the activity line is about, for example a persona of the product.
    public var activityIcon: UIImage? {
        didSet { if activityIcon !== oldValue { notify() } }
    }

    /// The title of the button that stops "Read aloud". Nil hides the button: the room speaks nothing.
    public var readAloudStopTitle: String? {
        didSet { if readAloudStopTitle != oldValue { notify() } }
    }

    /// The user pressed the button that stops "Read aloud".
    public var onStopReadAloud: (() -> Void)?

    public var suggestions: [Suggestion] = [] {
        didSet { if suggestions != oldValue { notify() } }
    }

    public var commands: [Command] = [] {
        didSet { if commands != oldValue { notify() } }
    }

    /// The user pressed a quick reply.
    public var onSuggestion: ((Suggestion) -> Void)?

    /// The room declares `input_events`: the quick replies stay while the user types, because the product suggests
    /// for the text, and each change of the text goes to `onInputText`.
    public var followsInput = false {
        didSet { if followsInput != oldValue { notify() } }
    }

    /// The text of the input changed. It is empty after the user sends.
    public var onInputText: ((String) -> Void)?

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

    /// The commands whose name starts with what the user typed after the slash, or no command when the text is not
    /// the start of a command.
    public func matchingCommands(for text: String) -> [Command] {
        guard text.hasPrefix("/"), !text.contains(" "), !text.contains("\n") else {
            return []
        }

        let prefix = text.dropFirst().lowercased()
        return commands.filter { $0.name.lowercased().hasPrefix(prefix) }
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
