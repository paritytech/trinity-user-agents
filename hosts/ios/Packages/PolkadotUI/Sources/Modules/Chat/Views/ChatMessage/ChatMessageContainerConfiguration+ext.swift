import UIKit
import DesignSystem
import ExternalAccessibility

private extension UIImage {
    var aspectRatio: CGFloat {
        guard size.height > 0 else { return 1 }
        return size.width / size.height
    }

    func predeterminedAspectRatio(tolerance: CGFloat = 0.05) -> CGFloat {
        let currentRatio = aspectRatio
        guard abs(currentRatio - 1.5) < tolerance else {
            return 1
        }
        return 1.5
    }
}

private extension ChatMessageContainerConfiguration {
    static func assertTextOrAttachments(
        text: String?,
        attachments: [ChatRichTextMessageConfiguration.AttachmentItem]?,
        productLinkPreview: ChatProductLinkPreviewConfiguration?
    ) {
        if text == nil, attachments == nil, productLinkPreview == nil {
            assertionFailure("Either text, attachments or productLinkPreview must be present")
        }
    }
}

public extension ChatMessageContainerConfiguration {
    static func outboxRichText(
        text: String?,
        statusConfiguration: ChatMessageStatusViewConfiguration?,
        attachments: [ChatRichTextMessageConfiguration.AttachmentItem]? = nil,
        productLinkPreview: ChatProductLinkPreviewConfiguration? = nil,
        replyInfo: ReplyInfo? = nil,
        canReply: Bool = true,
        addReaction: AddReactionViewModel? = nil,
        messageReaction: MessageReactionViewModel? = nil,
        layoutType: LayoutType = .plain
    ) -> Self {
        makeRichText(
            style: .outbox,
            text: text,
            statusConfiguration: statusConfiguration,
            attachments: attachments,
            productLinkPreview: productLinkPreview,
            replyInfo: replyInfo,
            canReply: canReply,
            addReaction: addReaction,
            messageReaction: messageReaction,
            layoutType: layoutType
        )
    }

    static func inboxRichText(
        text: String?,
        statusConfiguration: ChatMessageStatusViewConfiguration?,
        attachments: [ChatRichTextMessageConfiguration.AttachmentItem]? = nil,
        productLinkPreview: ChatProductLinkPreviewConfiguration? = nil,
        replyInfo: ReplyInfo? = nil,
        canReply: Bool = true,
        addReaction: AddReactionViewModel? = nil,
        messageReaction: MessageReactionViewModel? = nil,
        layoutType: LayoutType = .plain
    ) -> Self {
        makeRichText(
            style: .inbox,
            text: text,
            statusConfiguration: statusConfiguration,
            attachments: attachments,
            productLinkPreview: productLinkPreview,
            replyInfo: replyInfo,
            canReply: canReply,
            addReaction: addReaction,
            messageReaction: messageReaction,
            layoutType: layoutType
        )
    }

    static func inboxRichText(
        text: String?,
        bubbleColor: UIColor,
        textColor: UIColor,
        bubbleStrokeColor: UIColor? = nil,
        bubbleStrokeWidth: CGFloat = 0,
        statusConfiguration: ChatMessageStatusViewConfiguration?,
        attachments: [ChatRichTextMessageConfiguration.AttachmentItem]? = nil,
        productLinkPreview: ChatProductLinkPreviewConfiguration? = nil,
        replyInfo: ReplyInfo? = nil,
        canReply: Bool = true,
        addReaction: AddReactionViewModel? = nil,
        messageReaction: MessageReactionViewModel? = nil,
        layoutType: LayoutType = .plain
    ) -> Self {
        let style = RichTextStyle(side: .leading, bubbleColor: bubbleColor, textColor: textColor)
        return makeRichText(
            style: style,
            text: text,
            bubbleStrokeColor: bubbleStrokeColor,
            bubbleStrokeWidth: bubbleStrokeWidth,
            statusConfiguration: statusConfiguration,
            attachments: attachments,
            productLinkPreview: productLinkPreview,
            replyInfo: replyInfo,
            canReply: canReply,
            addReaction: addReaction,
            messageReaction: messageReaction,
            layoutType: layoutType
        )
    }

    static func botTextImage(
        text: String?,
        image: UIImage,
        layoutType: LayoutType = .plain,
        accessibilityId: (any AccessibilityIdentifying)? = nil
    ) -> Self {
        botTextImage(
            text: text,
            image: image,
            bubbleColor: .bgSurfaceContainer,
            bubbleStrokeColor: nil,
            bubbleStrokeWidth: 0,
            layoutType: layoutType,
            accessibilityId: accessibilityId
        )
    }

    static func botTextImage(
        text: String?,
        image: UIImage,
        bubbleColor: UIColor,
        bubbleStrokeColor: UIColor? = nil,
        bubbleStrokeWidth: CGFloat = 0,
        lockFullWidth: Bool = false,
        layoutType: LayoutType = .plain,
        accessibilityId: (any AccessibilityIdentifying)? = nil
    ) -> Self {
        let aspectRatio = image.predeterminedAspectRatio()
        let viewModel = ChatMessageTextImageCell.ViewModel(
            text: text,
            image: image,
            aspectRatio: aspectRatio,
            lockFullWidth: lockFullWidth,
            accessibilityId: accessibilityId
        )
        let view = ChatMessageTextImageCell(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)

        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .leading,
            bubbleColor: bubbleColor,
            bubbleStrokeColor: bubbleStrokeColor,
            bubbleStrokeWidth: bubbleStrokeWidth,
            canReply: false,
            contentInsets: .zero,
            layoutType: layoutType,
            identifier: ChatMessageTextImageCell.reuseIdentifier
        )
    }

    static func file(viewModel: ChatMessageFileCell.ViewModel, layoutType: LayoutType = .plain) -> Self {
        let view = ChatMessageFileCell(viewModel: viewModel)
        let configuration = SwiftUIContentConfiguration(view: view)
        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            canReply: false,
            contentInsets: .zero,
            layoutType: layoutType,
            identifier: ChatMessageFileCell.reuseIdentifier
        )
    }

    static func outboxSingleEmoji(
        emoji: String,
        statusConfiguration: ChatMessageStatusViewConfiguration?,
        replyInfo: ReplyInfo? = nil,
        canReply: Bool = true,
        addReaction: AddReactionViewModel? = nil,
        messageReaction: MessageReactionViewModel? = nil,
        layoutType: LayoutType = .plain,
    ) -> Self {
        let view = ChatMessageSingleEmojiView(emoji: emoji)
        let configuration = SwiftUIContentConfiguration(view: view)
        let hasAttachments = replyInfo != nil
        let contentInsets = hasAttachments
            ? RichTextLayout.emojiWithAttachmentsInsets
            : RichTextLayout.emojiOnlyInsets

        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            innerContentLayout: .sideAligned,
            side: .trailing,
            bubbleColor: .clear,
            statusConfiguration: statusConfiguration,
            replyInfo: replyInfo,
            canReply: canReply,
            addReaction: addReaction,
            messageReaction: messageReaction,
            contentInsets: contentInsets,
            statusViewInsets: RichTextLayout.emojiStatusInsets,
            layoutType: layoutType,
            identifier: ChatMessageSingleEmojiView.reuseIdentifier
        )
    }

    static func inboxSingleEmoji(
        emoji: String,
        statusConfiguration: ChatMessageStatusViewConfiguration?,
        replyInfo: ReplyInfo? = nil,
        canReply: Bool = true,
        addReaction: AddReactionViewModel? = nil,
        messageReaction: MessageReactionViewModel? = nil,
        layoutType: LayoutType = .plain,
    ) -> Self {
        let view = ChatMessageSingleEmojiView(emoji: emoji)
        let configuration = SwiftUIContentConfiguration(view: view)
        let hasAttachments = replyInfo != nil
        let contentInsets = hasAttachments
            ? RichTextLayout.emojiWithAttachmentsInsets
            : RichTextLayout.emojiOnlyInsets

        return ChatMessageContainerConfiguration(
            innerContent: configuration,
            innerContentLayout: .sideAligned,
            side: .leading,
            bubbleColor: .clear,
            statusConfiguration: statusConfiguration,
            statusLayout: .content,
            replyInfo: replyInfo,
            canReply: canReply,
            addReaction: addReaction,
            messageReaction: messageReaction,
            contentInsets: contentInsets,
            statusViewInsets: RichTextLayout.emojiInboxStatusInsets,
            layoutType: layoutType,
            identifier: ChatMessageSingleEmojiView.reuseIdentifier
        )
    }
}

private extension ChatMessageContainerConfiguration {
    enum RichTextLayout {
        static let emojiOnlyInsets = UIEdgeInsets(
            top: DSSpacings.tiny,
            left: DSSpacings.mediumIncreased,
            bottom: 20,
            right: DSSpacings.mediumIncreased
        )

        static let emojiWithAttachmentsInsets = UIEdgeInsets(
            top: -DSSpacings.extraTiny,
            left: DSSpacings.mediumIncreased,
            bottom: 20,
            right: DSSpacings.mediumIncreased
        )

        static let emojiStatusInsets = UIEdgeInsets(
            top: 0,
            left: 0,
            bottom: DSSpacings.tiny,
            right: DSSpacings.small
        )

        static let emojiInboxStatusInsets = UIEdgeInsets(
            top: 0,
            left: 0,
            bottom: DSSpacings.tiny,
            right: -DSSpacings.small
        )

        static let textOnlyInsets = UIEdgeInsets(
            top: DSSpacings.small + DSSpacings.tiny,
            left: DSSpacings.small + DSSpacings.tiny,
            bottom: DSSpacings.extraSmall,
            right: DSSpacings.small + DSSpacings.tiny
        )
        static let attachmentsOnlyInsets = UIEdgeInsets.zero
        static let attachmentsWithTextInsets = UIEdgeInsets(
            top: 0,
            left: 0,
            bottom: DSSpacings.small,
            right: 0
        )

        static let textOnlyStatusInsets = UIEdgeInsets(
            top: 0,
            left: 0,
            bottom: DSSpacings.extraSmall,
            right: DSSpacings.small
        )
        static let attachmentsOnlyStatusInsets = UIEdgeInsets(
            top: 0,
            left: 0,
            bottom: DSSpacings.small,
            right: DSSpacings.small
        )
        static let attachmentsWithTextStatusInsets = UIEdgeInsets(
            top: 0,
            left: 0,
            bottom: DSSpacings.extraSmall,
            right: DSSpacings.small
        )

        static let productLinkPreviewOnlyInsets = UIEdgeInsets(
            top: DSSpacings.extraTiny,
            left: DSSpacings.extraTiny,
            bottom: 30,
            right: DSSpacings.extraTiny
        )
        static let productLinkPreviewWithTextInsets = UIEdgeInsets(
            top: DSSpacings.extraTiny,
            left: DSSpacings.extraTiny,
            bottom: DSSpacings.extraSmall,
            right: DSSpacings.extraTiny
        )

        static let productLinkStatusInsets = UIEdgeInsets(
            top: 0,
            left: 0,
            bottom: DSSpacings.small,
            right: DSSpacings.small
        )
    }

    struct RichTextStyle {
        let side: ChatBubbleTailSide
        let bubbleColor: UIColor
        let textColor: UIColor

        static let outbox = RichTextStyle(
            side: .trailing,
            bubbleColor: .bgSurfaceContainerInverted,
            textColor: .fgPrimaryInverted
        )

        static let inbox = RichTextStyle(
            side: .leading,
            bubbleColor: .bgSurfaceContainer,
            textColor: .fgPrimary
        )
    }

    static func makeRichText(
        style: RichTextStyle,
        text: String?,
        bubbleStrokeColor: UIColor? = nil,
        bubbleStrokeWidth: CGFloat = 0,
        statusConfiguration: ChatMessageStatusViewConfiguration?,
        attachments: [ChatRichTextMessageConfiguration.AttachmentItem]?,
        productLinkPreview: ChatProductLinkPreviewConfiguration?,
        replyInfo: ReplyInfo?,
        canReply: Bool,
        addReaction: AddReactionViewModel?,
        messageReaction: MessageReactionViewModel?,
        layoutType: LayoutType
    ) -> Self {
        assertTextOrAttachments(text: text, attachments: attachments, productLinkPreview: productLinkPreview)

        let trimmedText = text?.isEmpty == false ? text : nil
        let hasAttachments = !(attachments?.isEmpty ?? true)
        let hasPreview = productLinkPreview != nil

        let contentInsets: UIEdgeInsets =
            switch (hasAttachments, hasPreview, trimmedText) {
            case (true, _, nil): RichTextLayout.attachmentsOnlyInsets
            case (true, _, _): RichTextLayout.attachmentsWithTextInsets
            case (false, true, nil): RichTextLayout.productLinkPreviewOnlyInsets
            case (false, true, _): RichTextLayout.productLinkPreviewWithTextInsets
            case (false, false, _): RichTextLayout.textOnlyInsets
            }

        let statusViewInsets: UIEdgeInsets =
            switch (hasAttachments, hasPreview, trimmedText) {
            case (true, _, nil): RichTextLayout.attachmentsOnlyStatusInsets
            case (true, _, _): RichTextLayout.attachmentsWithTextStatusInsets
            case (false, true, _): RichTextLayout.productLinkStatusInsets
            case (false, false, _): RichTextLayout.textOnlyStatusInsets
            }

        let innerContent = ChatRichTextMessageConfiguration(
            attachmentItems: attachments ?? [],
            textViewModel: trimmedText.map {
                .init(
                    text: $0,
                    textColor: style.textColor,
                    statusPlaceholderImage: statusConfiguration?.placeholderImage
                )
            },
            productLinkPreview: productLinkPreview
        )

        return ChatMessageContainerConfiguration(
            innerContent: innerContent,
            innerContentLayout: replyInfo != nil ? .leading : .fill,
            side: style.side,
            bubbleColor: style.bubbleColor,
            bubbleStrokeColor: bubbleStrokeColor,
            bubbleStrokeWidth: bubbleStrokeWidth,
            statusConfiguration: statusConfiguration,
            replyInfo: replyInfo,
            canReply: canReply,
            addReaction: addReaction,
            messageReaction: messageReaction,
            contentInsets: contentInsets,
            statusViewInsets: statusViewInsets,
            layoutType: layoutType,
            identifier: ChatMessageTextView.reuseIdentifier
        )
    }
}
