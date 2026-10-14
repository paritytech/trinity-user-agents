import UIKit
import DesignSystem

public extension ChatMessageStatusViewConfiguration {
    enum OutboxStatus {
        case pending
        case sent
        case delivered
        case failed

        public var image: UIImage {
            switch self {
            case .pending:
                UIImage(resource: .messagePending)
            case .sent:
                UIImage(resource: .messageSent)
            case .delivered:
                UIImage(resource: .messageDelivered)
            case .failed:
                UIImage(resource: .exclamationMark).withTintColor(.fgError, renderingMode: .alwaysOriginal)
            }
        }
    }

    static func inbox(
        date: Date?,
        formatter: TimestampFormatting,
        isEdited: Bool = false,
        background: Background? = nil
    ) -> ChatMessageStatusViewConfiguration {
        ChatMessageStatusViewConfiguration(
            dateFormatter: formatter,
            date: date,
            textColor: background == nil
                ? UIColor.fgTertiary
                : UIColor.fgStaticWhite,
            image: nil,
            isEdited: isEdited,
            background: background
        )
    }

    static func outbox(
        date: Date?,
        formatter: TimestampFormatting,
        status: OutboxStatus,
        isEdited: Bool = false,
        background: Background? = nil
    ) -> ChatMessageStatusViewConfiguration {
        ChatMessageStatusViewConfiguration(
            dateFormatter: formatter,
            date: date,
            textColor: background == nil
                ? UIColor.fgSecondaryInverted
                : UIColor.fgStaticWhite,
            image: background == nil
                ? status.image
                : status.image.withRenderingMode(.alwaysTemplate),
            isEdited: isEdited,
            background: background
        )
    }
}

public extension ChatMessageStatusViewConfiguration.Background {
    static var mediaOverlay: ChatMessageStatusViewConfiguration.Background {
        .init(
            color: UIColor.bgSurfaceOverlay,
            shape: .capsule,
            contentInsets: .init(top: 2, left: 8, bottom: 2, right: 8)
        )
    }
}

#if DEBUG
    extension ChatMessageStatusViewConfiguration {
        static var read: Self {
            .init(
                dateFormatter: TimestampFormatter(),
                date: .now,
                textColor: .fgPrimary,
                image: UIImage(resource: .messageDelivered),
                isEdited: false
            )
        }

        static var failed: Self {
            .outbox(date: .now, formatter: TimestampFormatter(), status: .failed)
        }
    }
#endif
