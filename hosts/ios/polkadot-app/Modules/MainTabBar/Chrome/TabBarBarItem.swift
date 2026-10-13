import ExternalAccessibility
import UIKit

/// One slot of the bar row as the bar shows it.
struct TabBarBarItem: Equatable {
    /// A `.tab` takes the selection when tapped; an `.action` opens a panel and never does.
    enum Role: Equatable {
        case tab
        case action
    }

    enum Content: Equatable {
        case icon(UIImage)
        case tabsGlyph(count: Int)
    }

    let content: Content
    let title: String?
    let role: Role
    let accessibilityLabel: String
    let accessibilityID: AccessibilityID.Tab?

    init(
        content: Content,
        title: String?,
        role: Role = .tab,
        accessibilityLabel: String,
        accessibilityID: AccessibilityID.Tab? = nil
    ) {
        self.content = content
        self.title = title
        self.role = role
        self.accessibilityLabel = accessibilityLabel
        self.accessibilityID = accessibilityID
    }

    init(
        icon: UIImage,
        title: String?,
        role: Role = .tab,
        accessibilityLabel: String,
        accessibilityID: AccessibilityID.Tab? = nil
    ) {
        self.init(
            content: .icon(icon),
            title: title,
            role: role,
            accessibilityLabel: accessibilityLabel,
            accessibilityID: accessibilityID
        )
    }
}
