import Foundation
import PolkadotUI

protocol TextMessageConfigurationBuilding {
    func build(context: TextMessageContext) -> ChatMessageContainerConfiguration
}

// MARK: - Factory

enum TextMessageConfigurationFactory {
    static func make(
        context: TextMessageContext,
        timeFormatter: TimestampFormatting
    ) -> ChatMessageContainerConfiguration {
        if (context.attachments?.isEmpty == false) || context.productLinkPreview != nil {
            makeRichText(context: context, timeFormatter: timeFormatter)
        } else {
            makePlainText(context: context, timeFormatter: timeFormatter)
        }
    }
}

private extension TextMessageConfigurationFactory {
    static func makeRichText(
        context: TextMessageContext,
        timeFormatter: TimestampFormatting
    ) -> ChatMessageContainerConfiguration {
        switch context.status {
        case .incoming:
            InboxTextConfigurationBuilder(timeFormatter: timeFormatter).build(context: context)
        case .outgoing:
            OutboxTextConfigurationBuilder(timeFormatter: timeFormatter).build(context: context)
        }
    }

    static func makePlainText(
        context: TextMessageContext,
        timeFormatter: TimestampFormatting
    ) -> ChatMessageContainerConfiguration {
        switch (context.status, context.text.isSingleEmoji) {
        case (.incoming, true):
            InboxSingleEmojiConfigurationBuilder(timeFormatter: timeFormatter).build(context: context)
        case (.incoming, false):
            InboxTextConfigurationBuilder(timeFormatter: timeFormatter).build(context: context)
        case (.outgoing, true):
            OutboxSingleEmojiConfigurationBuilder(timeFormatter: timeFormatter).build(context: context)
        case (.outgoing, false):
            OutboxTextConfigurationBuilder(timeFormatter: timeFormatter).build(context: context)
        }
    }
}

// MARK: - Builders

struct InboxTextConfigurationBuilder: TextMessageConfigurationBuilding {
    let timeFormatter: TimestampFormatting

    func build(context: TextMessageContext) -> ChatMessageContainerConfiguration {
        let statusConfig: ChatMessageStatusViewConfiguration? = (context.deliveryDate != nil || context.isEdited)
            ? .inbox(
                date: context.deliveryDate,
                formatter: timeFormatter,
                isEdited: context.isEdited,
                background: context.isAttachmentsOnly ? .mediaOverlay : nil
            )
            : nil

        if let style = context.inboxBubbleStyle {
            return .inboxRichText(
                text: context.text,
                bubbleColor: style.bubbleColor,
                textColor: style.textColor,
                bubbleStrokeColor: style.strokeColor,
                bubbleStrokeWidth: style.strokeWidth,
                statusConfiguration: statusConfig,
                attachments: context.attachments,
                productLinkPreview: context.productLinkPreview,
                replyInfo: context.replyInfo,
                canReply: context.canReply,
                addReaction: context.addReaction,
                messageReaction: context.messageReaction,
                layoutType: context.layoutContext.layoutType
            )
        }
        return .inboxRichText(
            text: context.text,
            statusConfiguration: statusConfig,
            attachments: context.attachments,
            productLinkPreview: context.productLinkPreview,
            replyInfo: context.replyInfo,
            canReply: context.canReply,
            addReaction: context.addReaction,
            messageReaction: context.messageReaction,
            layoutType: context.layoutContext.layoutType
        )
    }
}

struct InboxSingleEmojiConfigurationBuilder: TextMessageConfigurationBuilding {
    let timeFormatter: TimestampFormatting

    func build(context: TextMessageContext) -> ChatMessageContainerConfiguration {
        let statusConfig: ChatMessageStatusViewConfiguration? = (context.deliveryDate != nil || context.isEdited)
            ? .inbox(
                date: context.deliveryDate,
                formatter: timeFormatter,
                isEdited: context.isEdited,
                background: .mediaOverlay
            )
            : nil

        return .inboxSingleEmoji(
            emoji: context.text,
            statusConfiguration: statusConfig,
            replyInfo: context.replyInfo,
            canReply: context.canReply,
            addReaction: context.addReaction,
            messageReaction: context.messageReaction,
            layoutType: context.layoutContext.layoutType
        )
    }
}

struct OutboxTextConfigurationBuilder: TextMessageConfigurationBuilding {
    let timeFormatter: TimestampFormatting

    func build(context: TextMessageContext) -> ChatMessageContainerConfiguration {
        guard case let .outgoing(outgoingStatus) = context.status else {
            preconditionFailure("OutboxTextConfigurationBuilder requires outgoing status")
        }

        let statusConfig = ChatMessageStatusViewConfiguration.outbox(
            date: context.deliveryDate,
            formatter: timeFormatter,
            status: outgoingStatus.configurationStatus(),
            isEdited: context.isEdited,
            background: context.isAttachmentsOnly ? .mediaOverlay : nil
        )

        return .outboxRichText(
            text: context.text,
            statusConfiguration: statusConfig,
            attachments: context.attachments,
            productLinkPreview: context.productLinkPreview,
            replyInfo: context.replyInfo,
            canReply: context.canReply,
            addReaction: context.addReaction,
            messageReaction: context.messageReaction,
            layoutType: context.layoutContext.layoutType
        )
    }
}

struct OutboxSingleEmojiConfigurationBuilder: TextMessageConfigurationBuilding {
    let timeFormatter: TimestampFormatting

    func build(context: TextMessageContext) -> ChatMessageContainerConfiguration {
        guard case let .outgoing(outgoingStatus) = context.status else {
            preconditionFailure("OutboxSingleEmojiConfigurationBuilder requires outgoing status")
        }

        let statusConfig = ChatMessageStatusViewConfiguration.outbox(
            date: context.deliveryDate,
            formatter: timeFormatter,
            status: outgoingStatus.configurationStatus(),
            isEdited: context.isEdited,
            background: .mediaOverlay
        )

        return .outboxSingleEmoji(
            emoji: context.text,
            statusConfiguration: statusConfig,
            replyInfo: context.replyInfo,
            canReply: context.canReply,
            addReaction: context.addReaction,
            messageReaction: context.messageReaction,
            layoutType: context.layoutContext.layoutType
        )
    }
}
