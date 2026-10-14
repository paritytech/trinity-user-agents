import ExternalAccessibility
import UIKit

/// One slot of the bar row as the bar shows it.
struct TabBarBarItem: Equatable {
    enum Content: Equatable {
        case icon(UIImage)
        case tabsGlyph(count: Int)
    }

    let content: Content
    let title: String?
    let accessibilityLabel: String
    let accessibilityID: AccessibilityID.Tab?

    init(
        content: Content,
        title: String?,
        accessibilityLabel: String,
        accessibilityID: AccessibilityID.Tab? = nil
    ) {
        self.content = content
        self.title = title
        self.accessibilityLabel = accessibilityLabel
        self.accessibilityID = accessibilityID
    }

    init(
        icon: UIImage,
        title: String?,
        accessibilityLabel: String,
        accessibilityID: AccessibilityID.Tab? = nil
    ) {
        self.init(
            content: .icon(icon),
            title: title,
            accessibilityLabel: accessibilityLabel,
            accessibilityID: accessibilityID
        )
    }
}
