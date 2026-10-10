import UIKit

public struct DSTabBarItem: Equatable {
    /// A `.tab` takes the selection when tapped; an `.action` opens a panel and never does.
    public enum Role: Equatable {
        case tab
        case action
    }

    public enum Content: Equatable {
        case icon(UIImage)
        case tabsGlyph(count: Int)
    }

    public let content: Content
    public let title: String?
    public let role: Role
    public var accessibilityLabel: String
    public var accessibilityIdentifier: String?
    public var showsGlassBackground: Bool

    public init(
        content: Content,
        title: String?,
        role: Role = .tab,
        accessibilityLabel: String,
        accessibilityIdentifier: String? = nil,
        showsGlassBackground: Bool = false
    ) {
        self.content = content
        self.title = title
        self.role = role
        self.accessibilityLabel = accessibilityLabel
        self.accessibilityIdentifier = accessibilityIdentifier
        self.showsGlassBackground = showsGlassBackground
    }

    public init(
        icon: UIImage,
        title: String?,
        role: Role = .tab,
        accessibilityLabel: String,
        accessibilityIdentifier: String? = nil,
        showsGlassBackground: Bool = false
    ) {
        self.init(
            content: .icon(icon),
            title: title,
            role: role,
            accessibilityLabel: accessibilityLabel,
            accessibilityIdentifier: accessibilityIdentifier,
            showsGlassBackground: showsGlassBackground
        )
    }
}
