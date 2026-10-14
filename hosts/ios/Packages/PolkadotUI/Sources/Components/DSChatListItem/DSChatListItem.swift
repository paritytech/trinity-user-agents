import DesignSystem
import SwiftUI

public struct DSChatListItem<Avatar: View>: View {
    public struct Data: Hashable {
        public let sender: String
        public let timestamp: String
        public let message: String
        public let messageKind: DSChatMessage.Kind
        public let messageContext: DSChatMessage.Context
        public let isMuted: Bool
        public let hasReaction: Bool
        public let unreadCount: Int

        public init(
            sender: String,
            timestamp: String,
            message: String,
            messageKind: DSChatMessage.Kind = .default,
            messageContext: DSChatMessage.Context = .single,
            isMuted: Bool = false,
            hasReaction: Bool = false,
            unreadCount: Int = 0
        ) {
            self.sender = sender
            self.timestamp = timestamp
            self.message = message
            self.messageKind = messageKind
            self.messageContext = messageContext
            self.isMuted = isMuted
            self.hasReaction = hasReaction
            self.unreadCount = unreadCount
        }
    }

    private let data: Data
    private let avatar: Avatar
    private let action: (() -> Void)?

    public init(
        data: Data,
        action: (() -> Void)? = nil,
        @ViewBuilder avatar: () -> Avatar
    ) {
        self.data = data
        self.avatar = avatar()
        self.action = action
    }

    public var body: some View {
        if let action {
            Button(action: action) { rowContent }
                .buttonStyle(DSChatListItemButtonStyle())
        } else {
            rowContent
                .padding(DSSpacings.extraMedium)
        }
    }

    private var rowContent: some View {
        HStack(alignment: .top, spacing: DSSpacings.medium) {
            avatar
            messageSection
        }
    }

    private var messageSection: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            content
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var header: some View {
        HStack(alignment: .bottom, spacing: DSSpacings.small) {
            HStack(spacing: data.isMuted ? DSSpacings.small : 0) {
                Text(data.sender)
                    .typography(.titleMedium)
                    .foregroundStyle(Color.fgPrimary)
                    .lineLimit(1)
                if data.isMuted {
                    Image(.icon20SpeakerXMarkSolid)
                        .foregroundStyle(Color.fgTertiary)
                        .frame(
                            width: DSChatListItemMetrics.muteIconSize,
                            height: DSChatListItemMetrics.muteIconSize
                        )
                }
                Spacer(minLength: 0)
            }
            Text(data.timestamp)
                .typography(.bodyMedium.emphasized)
                .foregroundStyle(Color.fgTertiary)
                .lineLimit(1)
                .fixedSize()
        }
    }

    private var content: some View {
        HStack(alignment: .top, spacing: DSSpacings.small) {
            DSChatMessage(
                text: data.message,
                kind: data.messageKind,
                context: data.messageContext
            )
            .equatable()
            badges
        }
    }

    @ViewBuilder
    private var badges: some View {
        if data.hasReaction || data.unreadCount > 0 {
            HStack(spacing: DSSpacings.small) {
                if data.hasReaction {
                    DSChatListBadge(kind: .reaction, isMuted: data.isMuted)
                        .equatable()
                }
                if data.unreadCount > 0 {
                    DSChatListBadge(kind: .counter(data.unreadCount), isMuted: data.isMuted)
                        .equatable()
                }
            }
            .padding(.top, DSSpacings.extraSmall)
        }
    }
}

private enum DSChatListItemMetrics {
    static let muteIconSize: CGFloat = 20
}

private struct DSChatListItemButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(DSSpacings.extraMedium)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(configuration.isPressed ? Color.bgSelectionContainerActive : Color.clear)
            .contentShape(Rectangle())
    }
}

#if DEBUG
    #Preview("List") {
        VStack(spacing: 0) {
            DSChatListItem(
                data: .init(
                    sender: "mysticRiver.88",
                    timestamp: "12:30",
                    message: "Hey there! How can I assist you today?",
                    hasReaction: true,
                    unreadCount: 1
                ),
                action: {}
            ) {
                previewAvatar("M", bg: .avatarBgAmethyst, fg: .avatarFgAmethyst)
            }
            DSChatListItem(
                data: .init(
                    sender: "mysticRiver.88",
                    timestamp: "12:30",
                    message: "Hey there! How can I assist you today?",
                    isMuted: true,
                    hasReaction: true,
                    unreadCount: 1
                ),
                action: {}
            ) {
                previewAvatar("M", bg: .avatarBgAmethyst, fg: .avatarFgAmethyst)
            }
            DSChatListItem(
                data: .init(
                    sender: "team-chat",
                    timestamp: "Yesterday",
                    message: "Sent you a photo",
                    messageKind: .media,
                    messageContext: .group(senderName: "alice"),
                    unreadCount: 42
                )
            ) {
                previewAvatar("T", bg: .avatarBgEmerald, fg: .avatarFgEmerald)
            }
            DSChatListItem(
                data: .init(
                    sender: "bob.chain",
                    timestamp: "Mon",
                    message: "Voice call",
                    messageKind: .voiceCall,
                    unreadCount: 150
                )
            ) {
                previewAvatar("B", bg: .avatarBgRuby, fg: .avatarFgRuby)
            }
            DSChatListItem(
                data: .init(
                    sender: "councilDelegate",
                    timestamp: "12 May",
                    message: "Vote results posted in the referenda channel — please take a look when you have a moment.",
                    isMuted: true
                )
            ) {
                previewAvatar("C", bg: .avatarBgSapphire, fg: .avatarFgSapphire)
            }
        }
        .padding(.vertical, 8)
        .background(Color.bgSurfaceMain)
    }

    private func previewAvatar(_ letter: String, bg: Color, fg: Color) -> some View {
        DSLetterAvatar(letter: letter, background: bg, foreground: fg)
    }
#endif
