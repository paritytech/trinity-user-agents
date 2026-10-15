import Foundation
import UIKit

/// The texts and the choices of the mute and the archive actions of a product room, for the chat list and the
/// room header (07-LATER §6.2).
enum ChatMuteOptions {
    /// The sheet that asks how long to mute a room.
    static func sheet(onSelect: @escaping (Chat.MuteDuration) -> Void) -> UIAlertController {
        let sheet = UIAlertController(
            title: String(localized: .ChatExtension.roomMuteTitle),
            message: nil,
            preferredStyle: .actionSheet
        )
        for duration in Chat.MuteDuration.allCases {
            sheet.addAction(UIAlertAction(title: duration.title, style: .default) { _ in onSelect(duration) })
        }
        sheet.addAction(UIAlertAction(title: String(localized: .Common.cancel), style: .cancel))
        return sheet
    }

    /// The mute entry of a menu: a submenu of the durations, or "Unmute" for a muted room.
    static func menuElement(
        isMuted: Bool,
        apply: @escaping (ChatRoomUserStateChange) -> Void
    ) -> UIMenuElement {
        guard !isMuted else {
            return UIAction(title: String(localized: .ChatExtension.roomUnmute), image: unmuteImage) { _ in
                apply(.unmute)
            }
        }

        return UIMenu(
            title: String(localized: .ChatExtension.roomMute),
            image: muteImage,
            children: Chat.MuteDuration.allCases.map { duration in
                UIAction(title: duration.title) { _ in apply(.mute(duration)) }
            }
        )
    }

    /// The archive entry of a menu.
    static func archiveAction(
        isArchived: Bool,
        apply: @escaping (ChatRoomUserStateChange) -> Void
    ) -> UIAction {
        UIAction(title: archiveTitle(isArchived: isArchived), image: archiveImage(isArchived: isArchived)) { _ in
            apply(.archive(!isArchived))
        }
    }

    static func muteTitle(isMuted: Bool) -> String {
        String(localized: isMuted ? .ChatExtension.roomUnmute : .ChatExtension.roomMute)
    }

    static func archiveTitle(isArchived: Bool) -> String {
        String(localized: isArchived ? .ChatExtension.roomUnarchive : .ChatExtension.roomArchive)
    }

    static let muteImage = UIImage(systemName: "bell.slash")
    static let unmuteImage = UIImage(systemName: "bell")

    static func archiveImage(isArchived: Bool) -> UIImage? {
        UIImage(systemName: isArchived ? "tray.and.arrow.up" : "archivebox")
    }
}

private extension Chat.MuteDuration {
    var title: String {
        switch self {
        case .oneHour: String(localized: .ChatExtension.roomMuteHour)
        case .eightHours: String(localized: .ChatExtension.roomMuteHours)
        case .oneWeek: String(localized: .ChatExtension.roomMuteWeek)
        case .always: String(localized: .ChatExtension.roomMuteAlways)
        }
    }
}
