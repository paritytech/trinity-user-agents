import DesignSystem
import SwiftUI
import UIKit

// Chat text bubble matching Figma "Text Bubble" (42:2625) and mirroring the production
// renderer (ChatMessageContainerView / ChatMessageStatusView):
// - delivery status icons (pending / sent / delivered) shown only for outgoing messages,
// - the timestamp+status overlaps the last text line via a transparent inline placeholder,
// - corner-radius grouping for stacked messages,
// - reply reference quote with a leading accent.
public struct DSChatMessageBubble: View {
    public enum Sender {
        case me
        case other
    }

    // Mirrors ChatMessageStatusViewConfiguration.OutboxStatus.
    public enum DeliveryStatus {
        case pending
        case sent
        case delivered

        var icon: ImageResource {
            switch self {
            case .pending: .messagePending
            case .sent: .messageSent
            case .delivered: .messageDelivered
            }
        }
    }

    // Mirrors ChatMessageContainerConfiguration.LayoutType — controls which corners tighten
    // when messages from the same sender are stacked.
    public enum Grouping {
        case plain
        case groupedTop
        case groupedMiddle
        case groupedBottom
    }

    public struct Reference: Equatable {
        public var senderName: String
        public var text: String
        public var media: Image?

        public init(senderName: String, text: String, media: Image? = nil) {
            self.senderName = senderName
            self.text = text
            self.media = media
        }
    }

    public struct Reaction: Equatable {
        public var emoji: String
        public var isHighlighted: Bool

        public init(emoji: String, isHighlighted: Bool = false) {
            self.emoji = emoji
            self.isHighlighted = isHighlighted
        }
    }

    private let text: String
    private let sender: Sender
    private let timestamp: String?
    private let deliveryStatus: DeliveryStatus?
    private let isEdited: Bool
    private let reference: Reference?
    private let reactions: [Reaction]
    private let grouping: Grouping

    // Transparent inline run appended to the text so the last line reserves room for the
    // overlaid status, matching ChatMessageTextView.statusPlaceholderImage.
    private let statusReservation: UIImage?

    public init(
        text: String,
        sender: Sender,
        timestamp: String? = nil,
        deliveryStatus: DeliveryStatus? = nil,
        isEdited: Bool = false,
        reference: Reference? = nil,
        reactions: [Reaction] = [],
        grouping: Grouping = .plain
    ) {
        self.text = text
        self.sender = sender
        self.timestamp = timestamp
        self.deliveryStatus = deliveryStatus
        self.isEdited = isEdited
        self.reference = reference
        self.reactions = reactions
        self.grouping = grouping

        let showsCheck = sender == .me && deliveryStatus != nil
        statusReservation = Self.makeReservationImage(
            timestamp: timestamp,
            isEdited: isEdited,
            showsCheck: showsCheck
        )
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let reference {
                referenceView(reference)
            }
            textbox
        }
        .fixedSize(horizontal: false, vertical: true)
        .padding(DSSpacings.tiny)
        .background(sender.bubbleColor, in: bubbleShape)
        // Status is pinned to the bubble's bottom-right corner (as in ChatMessageContainerView),
        // not trailing the text — the inline placeholder keeps a long last line from running
        // under it. For short text in a wide bubble it sits at the corner with empty space.
        .overlay(alignment: .bottomTrailing) {
            if hasStatus {
                statusView
                    .padding(.trailing, DSChatMessageBubbleMetrics.statusCornerInset)
                    .padding(.bottom, DSChatMessageBubbleMetrics.statusCornerInset)
            }
        }
        .overlay(alignment: .bottomLeading) {
            reactionsView
        }
    }

    private var textbox: some View {
        (Text(text) + statusPlaceholder)
            .typography(.paragraphLarge)
            .foregroundStyle(sender.primaryForeground)
            .padding(DSSpacings.small)
    }

    private var statusPlaceholder: Text {
        guard let statusReservation else { return Text(verbatim: "") }
        return Text(Image(uiImage: statusReservation)).baselineOffset(-6)
    }

    private var hasStatus: Bool {
        timestamp != nil || isEdited || (sender == .me && deliveryStatus != nil)
    }

    private var statusView: some View {
        HStack(spacing: DSSpacings.tiny) {
            if isEdited {
                Text(String(localized: .messageEdited))
                    .typography(.bodySmall.emphasized)
                    .foregroundStyle(sender.secondaryForeground)
            }
            if let timestamp {
                Text(timestamp)
                    .typography(.bodySmall.emphasized)
                    .foregroundStyle(sender.secondaryForeground)
            }
            // Delivery status is the sender's own receipt — only outgoing messages show it.
            if sender == .me, let deliveryStatus {
                Image(deliveryStatus.icon)
                    .renderingMode(.template)
                    .resizable()
                    .scaledToFit()
                    .frame(
                        width: DSChatMessageBubbleMetrics.statusIconSize,
                        height: DSChatMessageBubbleMetrics.statusIconSize
                    )
                    .foregroundStyle(sender.secondaryForeground)
            }
        }
        .fixedSize()
    }

    private func referenceView(_ reference: Reference) -> some View {
        HStack(alignment: .top, spacing: DSSpacings.small) {
            if let media = reference.media {
                media
                    .resizable()
                    .scaledToFill()
                    .frame(width: DSChatMessageBubbleMetrics.mediaSize, height: DSChatMessageBubbleMetrics.mediaSize)
                    .clipShape(RoundedRectangle(cornerRadius: DSRadii.extraSmall, style: .continuous))
                    .overlay(
                        RoundedRectangle(cornerRadius: DSRadii.extraSmall, style: .continuous)
                            .stroke(Color.strokePrimaryInverted, lineWidth: 1)
                    )
            }
            VStack(alignment: .leading, spacing: 0) {
                Text(reference.senderName)
                    .typography(.titleTiny)
                    .foregroundStyle(sender.referenceForeground)
                    .lineLimit(1)
                Text(reference.text)
                    .typography(.bodySmall)
                    .foregroundStyle(sender.referenceForeground)
                    .lineLimit(2)
            }
        }
        .padding(.horizontal, DSSpacings.medium)
        .padding(.vertical, DSSpacings.small)
        .overlay(alignment: .leading) {
            Capsule()
                .fill(Color.strokeTertiary)
                .frame(width: DSChatMessageBubbleMetrics.referenceAccentWidth)
        }
        .background(sender.referenceColor)
        .clipShape(RoundedRectangle(cornerRadius: DSRadii.medium, style: .continuous))
    }

    @ViewBuilder
    private var reactionsView: some View {
        if !reactions.isEmpty {
            HStack(spacing: DSSpacings.small) {
                ForEach(Array(reactions.enumerated()), id: \.offset) { _, reaction in
                    Text(reaction.emoji)
                        .typography(.emojiSmall)
                        .padding(.horizontal, DSSpacings.small)
                        .padding(.vertical, DSSpacings.tiny)
                        .background(
                            reaction.isHighlighted ? Color.bgActionTertiary : Color.bgSurfaceContainer,
                            in: Capsule()
                        )
                        .overlay(Capsule().stroke(Color.strokePrimary, lineWidth: 1))
                }
            }
            .padding(.leading, DSSpacings.medium)
            .offset(y: DSChatMessageBubbleMetrics.reactionsOverlap)
        }
    }

    // Corner radii mirror ChatMessageContainerConfiguration.LayoutType.cornerRadii(for:):
    // grouped messages tighten the corners on the sender's side.
    private var bubbleShape: UnevenRoundedRectangle {
        let large = DSRadii.mediumIncreased
        let small = DSRadii.small
        var topLeading = large, bottomLeading = large, topTrailing = large, bottomTrailing = large

        switch (grouping, sender) {
        case (.plain, _):
            break
        case (.groupedTop, .other):
            bottomLeading = small
        case (.groupedTop, .me):
            bottomTrailing = small
        case (.groupedMiddle, .other):
            topLeading = small; bottomLeading = small
        case (.groupedMiddle, .me):
            topTrailing = small; bottomTrailing = small
        case (.groupedBottom, .other):
            topLeading = small
        case (.groupedBottom, .me):
            topTrailing = small
        }

        return UnevenRoundedRectangle(
            topLeadingRadius: topLeading,
            bottomLeadingRadius: bottomLeading,
            bottomTrailingRadius: bottomTrailing,
            topTrailingRadius: topTrailing,
            style: .continuous
        )
    }

    // Mirrors ChatMessageStatusViewConfiguration.estimatedWidth / placeholderImage.
    private static func makeReservationImage(
        timestamp: String?,
        isEdited: Bool,
        showsCheck: Bool
    ) -> UIImage? {
        let font = UIFont.systemFont(ofSize: 12)
        var width: CGFloat = 0

        if isEdited {
            let edited = String(localized: .messageEdited) as NSString
            width += edited.size(withAttributes: [.font: font]).width + DSSpacings.extraSmall
        }
        if let timestamp {
            width += (timestamp as NSString).size(withAttributes: [.font: font]).width + DSSpacings.tiny
        }
        if showsCheck {
            width += DSChatMessageBubbleMetrics.statusIconSize + DSSpacings.tiny
        }

        guard width > 0 else { return nil }

        let size = CGSize(width: width + DSChatMessageBubbleMetrics.reservationPadding, height: 14)
        return UIGraphicsImageRenderer(size: size).image { _ in }
    }
}

private enum DSChatMessageBubbleMetrics {
    static let statusIconSize: CGFloat = 12
    static let mediaSize: CGFloat = 48
    static let referenceAccentWidth: CGFloat = 4
    static let reactionsOverlap: CGFloat = 22
    static let reservationPadding: CGFloat = 8
    // Aligns the corner-pinned status with the text content (outer + textbox padding).
    static let statusCornerInset: CGFloat = DSSpacings.tiny + DSSpacings.small
}

private extension DSChatMessageBubble.Sender {
    var bubbleColor: Color {
        switch self {
        case .me: .bgSurfaceContainerInverted
        case .other: .bgSurfaceContainer
        }
    }

    var primaryForeground: Color {
        switch self {
        case .me: .fgPrimaryInverted
        case .other: .fgPrimary
        }
    }

    var secondaryForeground: Color {
        switch self {
        case .me: .fgSecondaryInverted
        case .other: .fgTertiary
        }
    }

    var referenceColor: Color {
        switch self {
        case .me: .bgSurfaceNestedInverted
        case .other: .bgSurfaceNested
        }
    }

    var referenceForeground: Color {
        switch self {
        case .me: .fgPrimaryInverted
        case .other: .fgPrimary
        }
    }
}

#if DEBUG
    @ViewBuilder
    private func dsChatBubblePreviewRow(
        _ sender: DSChatMessageBubble.Sender,
        @ViewBuilder _ bubble: () -> some View
    ) -> some View {
        HStack(spacing: 0) {
            switch sender {
            case .me:
                Spacer(minLength: DSSpacings.large)
                bubble()
            case .other:
                bubble()
                Spacer(minLength: DSSpacings.large)
            }
        }
    }

    private func dsChatBubblePreviewContainer(
        spacing: CGFloat = DSSpacings.large,
        @ViewBuilder _ content: () -> some View
    ) -> some View {
        VStack(alignment: .leading, spacing: spacing) {
            content()
        }
        .padding(DSSpacings.large)
        .frame(maxWidth: 360, alignment: .leading)
        .background(Color.bgSurfaceMain)
    }

    #Preview("Status & edited") {
        dsChatBubblePreviewContainer {
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(text: "Pending", sender: .me, timestamp: "9:01", deliveryStatus: .pending)
            }
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(text: "Sent · one check", sender: .me, timestamp: "9:02", deliveryStatus: .sent)
            }
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(
                    text: "Read · two checks",
                    sender: .me,
                    timestamp: "9:03",
                    deliveryStatus: .delivered
                )
            }
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(
                    text: "Edited and read",
                    sender: .me,
                    timestamp: "9:04",
                    deliveryStatus: .delivered,
                    isEdited: true
                )
            }
            dsChatBubblePreviewRow(.other) {
                DSChatMessageBubble(text: "Incoming · no check", sender: .other, timestamp: "9:05")
            }
            dsChatBubblePreviewRow(.other) {
                DSChatMessageBubble(text: "Incoming · edited", sender: .other, timestamp: "9:06", isEdited: true)
            }
        }
    }

    #Preview("Both sides · wrapping") {
        dsChatBubblePreviewContainer {
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(text: "Short", sender: .me, timestamp: "9:00", deliveryStatus: .sent)
            }
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(
                    text: "A longer outgoing message that wraps onto several lines so the status lands on the last line",
                    sender: .me,
                    timestamp: "9:01",
                    deliveryStatus: .delivered
                )
            }
            dsChatBubblePreviewRow(.other) {
                DSChatMessageBubble(text: "Short incoming", sender: .other, timestamp: "9:02")
            }
            dsChatBubblePreviewRow(.other) {
                DSChatMessageBubble(
                    text: "A longer incoming message that also wraps across a couple of lines",
                    sender: .other,
                    timestamp: "9:03"
                )
            }
        }
    }

    #Preview("Reply & media") {
        dsChatBubblePreviewContainer {
            dsChatBubblePreviewRow(.other) {
                DSChatMessageBubble(
                    text: "Of course! You?",
                    sender: .other,
                    timestamp: "12:40",
                    reference: .init(senderName: "You", text: "Hey, are you going to Web3 Summit? 🎉")
                )
            }
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(
                    text: "Replying with two checks",
                    sender: .me,
                    timestamp: "12:41",
                    deliveryStatus: .delivered,
                    reference: .init(
                        senderName: "Jake.23",
                        text: "Some longer referenced message that gets clamped to two lines"
                    )
                )
            }
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(
                    text: "Nice shot!",
                    sender: .me,
                    timestamp: "12:42",
                    deliveryStatus: .sent,
                    reference: .init(
                        senderName: "Jake.23",
                        text: "Check out this photo",
                        media: Image(systemName: "photo.fill")
                    )
                )
            }
            dsChatBubblePreviewRow(.other) {
                DSChatMessageBubble(
                    text: "Look at this",
                    sender: .other,
                    timestamp: "12:43",
                    reference: .init(senderName: "You", text: "Shared a photo", media: Image(systemName: "photo.fill"))
                )
            }
        }
    }

    #Preview("Reactions") {
        dsChatBubblePreviewContainer(spacing: DSSpacings.extraLargeIncreased) {
            dsChatBubblePreviewRow(.me) {
                DSChatMessageBubble(
                    text: "React to me",
                    sender: .me,
                    timestamp: "1:00",
                    deliveryStatus: .delivered,
                    reactions: [.init(emoji: "🙏"), .init(emoji: "👍", isHighlighted: true), .init(emoji: "💖")]
                )
            }
            dsChatBubblePreviewRow(.other) {
                DSChatMessageBubble(
                    text: "React to them",
                    sender: .other,
                    timestamp: "1:01",
                    reactions: [.init(emoji: "🎉"), .init(emoji: "🔥", isHighlighted: true)]
                )
            }
        }
    }

    #Preview("Grouping") {
        dsChatBubblePreviewContainer(spacing: DSSpacings.large) {
            VStack(alignment: .trailing, spacing: 2) {
                DSChatMessageBubble(
                    text: "Grouped top",
                    sender: .me,
                    timestamp: "1:00",
                    deliveryStatus: .sent,
                    grouping: .groupedTop
                )
                DSChatMessageBubble(
                    text: "Grouped middle",
                    sender: .me,
                    timestamp: "1:01",
                    deliveryStatus: .sent,
                    grouping: .groupedMiddle
                )
                DSChatMessageBubble(
                    text: "Grouped bottom",
                    sender: .me,
                    timestamp: "1:02",
                    deliveryStatus: .delivered,
                    grouping: .groupedBottom
                )
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
            VStack(alignment: .leading, spacing: 2) {
                DSChatMessageBubble(text: "Grouped top", sender: .other, timestamp: "1:03", grouping: .groupedTop)
                DSChatMessageBubble(text: "Grouped middle", sender: .other, timestamp: "1:04", grouping: .groupedMiddle)
                DSChatMessageBubble(text: "Grouped bottom", sender: .other, timestamp: "1:05", grouping: .groupedBottom)
            }
        }
    }
#endif
