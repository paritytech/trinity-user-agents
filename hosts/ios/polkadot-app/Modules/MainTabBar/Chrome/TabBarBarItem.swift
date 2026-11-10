import ExternalAccessibility
import UIKit

/// One slot of the bar row as the bar shows it.
struct TabBarBarItem: Equatable {
    enum Content: Equatable {
        case icon(UIImage)
        case tabsGlyph(count: Int)
    }

    let content: Content
    let title: LocalizedStringResource?
    let accessibilityLabel: LocalizedStringResource
    let accessibilityID: AccessibilityID.Tab?
    let accessibilityValue: LocalizedStringResource?

    init(
        content: Content,
        title: LocalizedStringResource?,
        accessibilityLabel: LocalizedStringResource,
        accessibilityID: AccessibilityID.Tab? = nil,
        accessibilityValue: LocalizedStringResource? = nil
    ) {
        self.content = content
        self.title = title
        self.accessibilityLabel = accessibilityLabel
        self.accessibilityID = accessibilityID
        self.accessibilityValue = accessibilityValue
    }

    init(
        icon: UIImage,
        title: LocalizedStringResource?,
        accessibilityLabel: LocalizedStringResource,
        accessibilityID: AccessibilityID.Tab? = nil,
        accessibilityValue: LocalizedStringResource? = nil
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
