import UIKit

// MARK: - ButtonConfiguration

public extension ChatMessageMediaViewConfiguration {
    struct ButtonConfiguration: Hashable {
        let style: ChatMessageMediaButtonStyle
        let size: ChatMessageMediaButtonSize
        let action: () -> Void

        public init(
            style: ChatMessageMediaButtonStyle,
            size: ChatMessageMediaButtonSize = .large,
            action: @escaping () -> Void = {}
        ) {
            self.style = style
            self.size = size
            self.action = action
        }

        public static func == (
            lhs: ButtonConfiguration,
            rhs: ButtonConfiguration
        ) -> Bool {
            lhs.style == rhs.style &&
                lhs.size == rhs.size
        }

        public func hash(into hasher: inout Hasher) {
            hasher.combine(style)
        }
    }
}

// MARK: - Static field → provider conversions

public extension ChatMessageMediaViewConfiguration.ButtonConfiguration {
    func asProvider() -> any ChatMessageMediaButtonConfigurationProviding {
        StaticChatMessageMediaButtonConfigurationProvider(self)
    }
}
