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
    let accessibilityValue: String?

    init(
        content: Content,
        title: String?,
        accessibilityLabel: String,
        accessibilityID: AccessibilityID.Tab? = nil,
        accessibilityValue: String? = nil
    ) {
        self.content = content
        self.title = title
        self.accessibilityLabel = accessibilityLabel
        self.accessibilityID = accessibilityID
        self.accessibilityValue = accessibilityValue
    }

    init(
        icon: UIImage,
        title: String?,
        accessibilityLabel: String,
        accessibilityID: AccessibilityID.Tab? = nil,
        accessibilityValue: String? = nil
    ) {
        self.init(
            content: .icon(icon),
            title: title,
            accessibilityLabel: accessibilityLabel,
            accessibilityID: accessibilityID,
            accessibilityValue: accessibilityValue
        )
    }
}
