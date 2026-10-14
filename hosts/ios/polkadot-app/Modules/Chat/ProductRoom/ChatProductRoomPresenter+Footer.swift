import ExternalAccessibility
import Foundation
import PolkadotUI

/// The footer of a product room (P6.6, 02-DESIGN §6.6, scenarios S33 to S35): the input as the product configures
/// it, no input, or the tree that the product draws.
extension ChatProductRoomPresenter {
    /// The longest text that the input of a product room takes. The core limits a text body to 16 KiB.
    static let inputCharacterLimit = 4_096

    /// The footer configuration of the room, or nil before the room has its policy.
    func inputConfiguration() -> (any ChatInputViewConfigurationProtocol)? {
        guard let policy else {
            return nil
        }

        switch policy.footer {
        case let .input(input):
            return ChatProductInputConfiguration(
                body: .input(Self.inputViewConfiguration(input), onAttach: { [weak self] in
                    self?.showAttachmentPicker()
                }),
                accessory: accessory
            )
        case .empty:
            return ChatProductInputConfiguration(body: .empty, accessory: accessory)
        case .custom:
            return ChatProductInputConfiguration(body: .custom(customFooterView()), accessory: accessory)
        }
    }

    /// The host input with the hint and the attachment button that the product configures. This host has no
    /// payment path in a product room yet, so the input shows no payment button (P10).
    static func inputViewConfiguration(_ input: ChatRoomPolicy.Input) -> ChatInputViewConfiguration {
        ChatInputViewConfiguration(
            placeholder: input.placeholder ?? ChatInputViewConfiguration.defaultPlaceholder,
            maxNumberOfLines: 7,
            maxCharacterCount: inputCharacterLimit,
            canPay: false,
            canAttachFile: !input.attachmentKinds.isEmpty,
            canSendWithoutText: false,
            inputAccessibilityId: AccessibilityID.Chat.messageInput,
            sendAccessibilityId: AccessibilityID.Chat.sendButton
        )
    }

    func customFooterView() -> ProductWidgetChatView {
        let widget = footerWidget ?? ProductFooterWidgetViewModel(roomId: roomId, renderer: footerRenderer)
        footerWidget = widget

        return ProductWidgetChatView(
            messageId: "footer:" + roomId,
            nodeProvider: widget,
            onAction: { [weak self] actionId, payload in
                guard let self else { return }
                Task { [footerRenderer, roomId] in
                    await footerRenderer.dispatchFooterAction(
                        roomId: roomId,
                        actionId: actionId,
                        payload: payload
                    )
                }
            },
            resolveImage: footerImageResolver
        )
    }
}

extension ChatRoomPolicy.Input {
    /// The kinds of attachment that the input offers, in the order of its menu.
    var attachmentKinds: [ChatAttachmentKind] {
        var kinds: [ChatAttachmentKind] = []
        if allowsPhoto { kinds.append(.photo) }
        if allowsVideo { kinds.append(.video) }
        if allowsFile { kinds.append(.file) }
        return kinds
    }
}

/// A kind of attachment that the input of a product room offers.
enum ChatAttachmentKind: Equatable, CaseIterable {
    case photo
    case video
    case file
}
