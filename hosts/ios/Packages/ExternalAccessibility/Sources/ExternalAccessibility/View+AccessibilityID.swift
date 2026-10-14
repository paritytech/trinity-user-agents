import SwiftUI

public extension View {
    func accessibilityId(_ id: (any AccessibilityIdentifying)?) -> some View {
        accessibilityIdentifier(id?.rawValue ?? "")
    }

    func accessibilityId(rawValue: String?) -> some View {
        accessibilityIdentifier(rawValue ?? "")
    }
}
