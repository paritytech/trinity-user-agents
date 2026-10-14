import UIKit

public struct DSTabBarItem: Equatable {
    /// A `.tab` participates in selection and carries the lens; an `.action` triggers a panel and
    /// is skipped by drag resolution.
    public enum Role: Equatable {
        case tab
        case action
    }

    public enum Content: Equatable {
        case icon(UIImage)
        case tabsGlyph(count: Int)
    }

    public enum Badge: Equatable {
        case attention

        var color: UIColor {
            switch self {
            case .attention: .bgStatusWarning
            }
        }
    }

    public let content: Content
    public let title: String?
    public let role: Role
    public var badge: Badge?
    public var accessibilityLabel: String
    public var accessibilityIdentifier: String?
    public var showsGlassBackground: Bool

    public init(
        content: Content,
        title: String?,
        role: Role = .tab,
        badge: Badge? = nil,
        accessibilityLabel: String,
        accessibilityIdentifier: String? = nil,
        showsGlassBackground: Bool = false
    ) {
        self.content = content
        self.title = title
        self.role = role
        self.badge = badge
        self.accessibilityLabel = accessibilityLabel
        self.accessibilityIdentifier = accessibilityIdentifier
        self.showsGlassBackground = showsGlassBackground
    }

    public init(
        icon: UIImage,
        title: String?,
        role: Role = .tab,
        badge: Badge? = nil,
        accessibilityLabel: String,
        accessibilityIdentifier: String? = nil,
        showsGlassBackground: Bool = false
    ) {
        self.init(
            content: .icon(icon),
            title: title,
            role: role,
            badge: badge,
            accessibilityLabel: accessibilityLabel,
            accessibilityIdentifier: accessibilityIdentifier,
            showsGlassBackground: showsGlassBackground
        )
    }
}
