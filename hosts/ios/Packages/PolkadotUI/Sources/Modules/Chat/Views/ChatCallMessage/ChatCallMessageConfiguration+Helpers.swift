import DesignSystem
import UIKit

// MARK: - TailSide + Bubble Tail Corner

extension ChatBubbleTailSide {
    var tailCorner: UIRectCorner {
        switch self {
        case .leading: .bottomLeft
        case .trailing: .bottomRight
        }
    }
}

// MARK: - Direction + Appearance

extension ChatCallMessageConfiguration.Direction {
    var side: ChatBubbleTailSide {
        switch self {
        case .incoming: .leading
        case .outgoing: .trailing
        }
    }

    var backgroundColor: UIColor {
        switch self {
        case .incoming: .bgSurfaceContainer
        case .outgoing: .bgSurfaceContainerInverted
        }
    }

    var titleColor: UIColor {
        switch self {
        case .incoming: .fgPrimary
        case .outgoing: .fgPrimaryInverted
        }
    }

    var subtitleColor: UIColor {
        switch self {
        case .incoming: .fgTertiary
        case .outgoing: .fgSecondaryInverted
        }
    }

    var iconContainerColor: UIColor {
        switch self {
        case .incoming: .bgSurfaceNested
        case .outgoing: .bgSurfaceNestedInverted
        }
    }
}

// MARK: - Configuration + Combined Appearance

extension ChatCallMessageConfiguration {
    var iconTintColor: UIColor {
        switch direction {
        case .incoming: .fgPrimary
        case .outgoing: .fgPrimaryInverted
        }
    }

    var title: String {
        Self.title(direction: direction, callType: callType, state: state)
    }

    public static func title(
        direction: Direction,
        callType: CallType,
        state: State
    ) -> String {
        let isAudio = callType == .audio
        switch (direction, state) {
        case (.incoming, .calling):
            return isAudio ? String(localized: .chatCallIncomingAudio) : String(localized: .chatCallIncomingVideo)
        case (.outgoing, .calling):
            return isAudio ? String(localized: .chatCallOutgoingAudio) : String(localized: .chatCallOutgoingVideo)
        case (_, .active):
            return isAudio ? String(localized: .chatCallActiveAudio) : String(localized: .chatCallActiveVideo)
        case (_, .finished):
            return isAudio ? String(localized: .chatCallFinishedAudio) : String(localized: .chatCallFinishedVideo)
        case (_, .missed):
            return isAudio ? String(localized: .chatCallMissedAudio) : String(localized: .chatCallMissedVideo)
        case (_, .cancelled):
            return isAudio ? String(localized: .chatCallCancelledAudio) : String(localized: .chatCallCancelledVideo)
        }
    }
}

// MARK: - State + Subtitle

extension ChatCallMessageConfiguration.State {
    var subtitle: String {
        switch self {
        case .calling:
            String(localized: .chatCallSubtitleCalling)
        case .active:
            String(localized: .chatCallSubtitleActive)
        case let .finished(duration):
            duration
        case .missed:
            String(localized: .chatCallSubtitleMissed)
        case let .cancelled(ringDuration):
            ringDuration
        }
    }

    var subtitleIcon: UIImage {
        switch self {
        case .calling,
             .active,
             .finished:
            UIImage(resource: .arrowTopRight)
        case .missed,
             .cancelled:
            UIImage(resource: .arrowBottomLeft)
        }
    }

    var subtitleIconTint: UIColor {
        switch self {
        case .calling,
             .active,
             .finished:
            .fgSuccess
        case .missed:
            .fgError
        case .cancelled:
            .fgTertiary
        }
    }
}

// MARK: - CallType + Icon

extension ChatCallMessageConfiguration.CallType {
    var icon: UIImage {
        switch self {
        case .audio: UIImage(resource: .icon16PhoneCall)
        case .video: UIImage(resource: .icon16Video)
        }
    }
}
