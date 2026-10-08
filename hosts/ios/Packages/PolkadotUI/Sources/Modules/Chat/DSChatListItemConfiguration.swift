import DesignSystem
import ExternalAccessibility
import SwiftUI
import UIKit

public struct DSChatListItemConfiguration: HashableContentConfiguration {
    /// An image loaded over the letter avatar. `id` names the image, so rows compare without loading it.
    public struct AvatarIcon {
        public let id: String
        public let viewModel: any ImageViewModelProtocol

        public init(id: String, viewModel: any ImageViewModelProtocol) {
            self.id = id
            self.viewModel = viewModel
        }
    }

    public let avatarViewModel: AvatarViewModel
    public let avatarIcon: AvatarIcon?
    public let sender: String
    public let message: String?
    public let messageKind: DSChatMessage.Kind
    public let messageContext: DSChatMessage.Context
    public let date: Date?
    public let isMuted: Bool
    public let hasReaction: Bool
    public let unreadCount: Int
    public let accessibilityId: String?

    let dateFormatter: TimestampFormatting

    public init(
        dateFormatter: TimestampFormatting,
        avatarViewModel: AvatarViewModel,
        avatarIcon: AvatarIcon? = nil,
        sender: String,
        message: String? = nil,
        messageKind: DSChatMessage.Kind = .default,
        messageContext: DSChatMessage.Context = .single,
        date: Date? = nil,
        isMuted: Bool = false,
        hasReaction: Bool = false,
        unreadCount: Int = 0,
        accessibilityId: (any AccessibilityIdentifying)? = nil
    ) {
        self.dateFormatter = dateFormatter
        self.avatarViewModel = avatarViewModel
        self.avatarIcon = avatarIcon
        self.sender = sender
        self.message = message
        self.messageKind = messageKind
        self.messageContext = messageContext
        self.date = date
        self.isMuted = isMuted
        self.hasReaction = hasReaction
        self.unreadCount = unreadCount
        self.accessibilityId = accessibilityId?.rawValue
    }

    public func makeContentView() -> any UIView & UIContentView {
        UIHostingConfiguration {
            DSChatListItem(data: rowData) {
                DSAvatarFactory.chatList(avatarViewModel, icon: avatarIcon?.viewModel)
            }
            .accessibilityId(rawValue: accessibilityId)
        }
        .margins(.all, 0)
        .makeContentView()
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(avatarViewModel)
        hasher.combine(avatarIcon?.id)
        hasher.combine(sender)
        hasher.combine(message)
        hasher.combine(messageKind)
        hasher.combine(messageContext)
        hasher.combine(date)
        hasher.combine(isMuted)
        hasher.combine(hasReaction)
        hasher.combine(unreadCount)
        hasher.combine(accessibilityId)
    }

    public static func == (lhs: DSChatListItemConfiguration, rhs: DSChatListItemConfiguration) -> Bool {
        lhs.avatarViewModel == rhs.avatarViewModel &&
            lhs.avatarIcon?.id == rhs.avatarIcon?.id &&
            lhs.sender == rhs.sender &&
            lhs.message == rhs.message &&
            lhs.messageKind == rhs.messageKind &&
            lhs.messageContext == rhs.messageContext &&
            lhs.date == rhs.date &&
            lhs.isMuted == rhs.isMuted &&
            lhs.hasReaction == rhs.hasReaction &&
            lhs.unreadCount == rhs.unreadCount &&
            lhs.accessibilityId == rhs.accessibilityId
    }

    private var rowData: DSChatListItem<DSAsyncAvatar>.Data {
        DSChatListItem<DSAsyncAvatar>.Data(
            sender: sender,
            timestamp: formattedDate ?? "",
            message: message ?? "",
            messageKind: messageKind,
            messageContext: messageContext,
            isMuted: isMuted,
            hasReaction: hasReaction,
            unreadCount: unreadCount
        )
    }

    private var formattedDate: String? {
        date.map { dateFormatter.string(for: $0) }
    }
}
