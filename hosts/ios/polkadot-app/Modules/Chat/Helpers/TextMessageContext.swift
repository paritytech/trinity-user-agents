import Foundation
import UIKit
import PolkadotUI

struct TextMessageContext {
    let text: String
    let attachments: [ChatRichTextMessageConfiguration.AttachmentItem]?
    let productLinkPreview: ChatProductLinkPreviewConfiguration?
    let status: Chat.LocalMessage.Status
    let deliveryDate: Date?
    let layoutContext: MessageLayoutContext
    let replyInfo: ChatMessageContainerConfiguration.ReplyInfo?
    let addReaction: ChatMessageContainerConfiguration.AddReactionViewModel?
    let messageReaction: ChatMessageContainerConfiguration.MessageReactionViewModel?
    let isEdited: Bool
    let canReply: Bool
    let inboxBubbleStyle: InboxBubbleStyle?

    init(
        text: String,
        attachments: [ChatRichTextMessageConfiguration.AttachmentItem]?,
        productLinkPreview: ChatProductLinkPreviewConfiguration?,
        status: Chat.LocalMessage.Status,
        deliveryDate: Date?,
        layoutContext: MessageLayoutContext,
        replyInfo: ChatMessageContainerConfiguration.ReplyInfo?,
        addReaction: ChatMessageContainerConfiguration.AddReactionViewModel?,
        messageReaction: ChatMessageContainerConfiguration.MessageReactionViewModel?,
        isEdited: Bool,
        canReply: Bool,
        inboxBubbleStyle: InboxBubbleStyle? = nil
    ) {
        self.text = text
        self.attachments = attachments
        self.productLinkPreview = productLinkPreview
        self.status = status
        self.deliveryDate = deliveryDate
        self.layoutContext = layoutContext
        self.replyInfo = replyInfo
        self.addReaction = addReaction
        self.messageReaction = messageReaction
        self.isEdited = isEdited
        self.canReply = canReply
        self.inboxBubbleStyle = inboxBubbleStyle
    }

    var isAttachmentsOnly: Bool {
        text.isEmpty && (attachments?.isEmpty == false)
    }
}

struct InboxBubbleStyle {
    let bubbleColor: UIColor
    let textColor: UIColor
    let strokeColor: UIColor?
    let strokeWidth: CGFloat

    init(
        bubbleColor: UIColor,
        textColor: UIColor,
        strokeColor: UIColor? = nil,
        strokeWidth: CGFloat = 0
    ) {
        self.bubbleColor = bubbleColor
        self.textColor = textColor
        self.strokeColor = strokeColor
        self.strokeWidth = strokeWidth
    }
}
