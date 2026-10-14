import UIKit

extension DSIconButton {
    static var chatInputSend: DSIconButton {
        DSIconButton(
            style: .primary,
            shape: .pill,
            size: .extraSmall,
            icon: UIImage(resource: .sendMessage)
        )
    }

    static var chatInputReplyClose: DSIconButton {
        DSIconButton(
            style: .ghost,
            shape: .pill,
            size: .tiny,
            icon: UIImage(resource: .buttonClose)
        )
    }

    // Glass on iOS 26 (interactive press); falls back to the `.secondary` fill below.
    static func chatInputLeading(icon: UIImage) -> DSIconButton {
        DSIconButton(
            style: .ghost,
            shape: .pill,
            size: .mediumIncreased,
            icon: icon,
            glass: false
        )
    }

    static var chatScrollToBottom: DSIconButton {
        DSIconButton(
            style: .secondary,
            shape: .pill,
            size: .mediumIncreased,
            icon: UIImage(resource: .chevronDown24),
            glass: true
        )
    }

    static var chatScrollToReaction: DSIconButton {
        DSIconButton(
            style: .secondary,
            shape: .pill,
            size: .mediumIncreased,
            icon: UIImage(resource: .heartOutline24),
            glass: true
        )
    }
}
