import UIKit

/// The footer of a product room (Chat Extension v2, 02-DESIGN §6.6): the input as the product configures it, no
/// input, or the tree that the product draws, with the activity line, the quick replies and the slash command
/// catalogue above it.
public struct ChatProductInputConfiguration: ChatInputViewConfigurationProtocol {
    public enum Body {
        /// The host input. `onAttach` runs when the user presses the attachment button.
        case input(ChatInputViewConfiguration, onAttach: (() -> Void)?)
        /// No input: the messages fill the screen.
        case empty
        /// The tree that the product draws, at most 40 % of the screen high.
        case custom(ProductWidgetChatView)
    }

    public let body: Body
    public let accessory: ChatRoomAccessoryModel

    public init(body: Body, accessory: ChatRoomAccessoryModel) {
        self.body = body
        self.accessory = accessory
    }

    public var activateOnAppear: Bool {
        false
    }

    public func makeContentView(for handler: ChatInputHandling?) -> UIView {
        ChatProductInputView(configuration: self, handler: handler)
    }

    public func equalsTo(configuration: any ChatInputViewConfigurationProtocol) -> Bool {
        guard let other = configuration as? ChatProductInputConfiguration, other.accessory === accessory else {
            return false
        }

        switch (body, other.body) {
        case let (.input(lhs, _), .input(rhs, _)): return lhs == rhs
        case (.empty, .empty): return true
        case let (.custom(lhs), .custom(rhs)): return lhs == rhs
        default: return false
        }
    }
}
