import ExternalAccessibility
import UIKit

public struct ChatInputViewConfiguration: ChatInputViewConfigurationProtocol, Equatable {
    let placeholder: String
    let maxNumberOfLines: Int
    let maxCharacterCount: Int
    let showsCharacterCounter: Bool
    let canPay: Bool
    let canAttachFile: Bool
    let canSendWithoutText: Bool
    let inputAccessibilityId: String?
    let sendAccessibilityId: String?

    public init(
        placeholder: String,
        maxNumberOfLines: Int,
        maxCharacterCount: Int,
        showsCharacterCounter: Bool = false,
        canPay: Bool,
        canAttachFile: Bool,
        canSendWithoutText: Bool,
        inputAccessibilityId: (any AccessibilityIdentifying)? = nil,
        sendAccessibilityId: (any AccessibilityIdentifying)? = nil
    ) {
        self.placeholder = placeholder
        self.maxNumberOfLines = maxNumberOfLines
        self.maxCharacterCount = maxCharacterCount
        self.showsCharacterCounter = showsCharacterCounter
        self.canPay = canPay
        self.canAttachFile = canAttachFile
        self.canSendWithoutText = canSendWithoutText
        self.inputAccessibilityId = inputAccessibilityId?.rawValue
        self.sendAccessibilityId = sendAccessibilityId?.rawValue
    }

    public var activateOnAppear: Bool {
        false
    }

    public func makeContentView(
        for handler: ChatInputHandling?
    ) -> UIView {
        DSChatInputView(configuration: self, handler: handler)
    }

    public func equalsTo(
        configuration: any ChatInputViewConfigurationProtocol
    ) -> Bool {
        guard
            let otherChatInputConfig = configuration as? ChatInputViewConfiguration else {
            return false
        }

        return self == otherChatInputConfig
    }

    public static func chat(
        canPay: Bool,
        canAttachFile: Bool,
        canSendWithoutText: Bool = false,
        inputAccessibilityId: (any AccessibilityIdentifying)? = AccessibilityID.Chat.messageInput,
        sendAccessibilityId: (any AccessibilityIdentifying)? = AccessibilityID.Chat.sendButton
    ) -> Self {
        .init(
            placeholder: String(localized: .chatInputPlaceholder),
            maxNumberOfLines: 7,
            maxCharacterCount: 500,
            canPay: canPay,
            canAttachFile: canAttachFile,
            canSendWithoutText: canSendWithoutText,
            inputAccessibilityId: inputAccessibilityId,
            sendAccessibilityId: sendAccessibilityId
        )
    }

    /// The welcome message of a chat request, which must fit one notification statement.
    public static func chatRequest(
        inputAccessibilityId: (any AccessibilityIdentifying)? = AccessibilityID.Chat.messageInput,
        sendAccessibilityId: (any AccessibilityIdentifying)? = AccessibilityID.Chat.sendButton
    ) -> Self {
        .init(
            placeholder: String(localized: .chatInputPlaceholder),
            maxNumberOfLines: 7,
            maxCharacterCount: 1_000,
            showsCharacterCounter: true,
            canPay: false,
            canAttachFile: false,
            canSendWithoutText: false,
            inputAccessibilityId: inputAccessibilityId,
            sendAccessibilityId: sendAccessibilityId
        )
    }
}
