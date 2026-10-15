import DesignSystem
import SwiftUI

public extension View {
    /// Draws the bar's selection pill behind the view when selected, sharing the container's matched geometry so
    /// the one pill travels between the bar and this view. Only has effect inside a `DSTabBarContainer`. The caller
    /// must ensure the bar's `selectedSlot` is nil while any view here is selected.
    func dsTabBarSelectionPill(isSelected: Bool) -> some View {
        modifier(DSTabBarSelectionPillModifier(isSelected: isSelected))
    }
}

extension EnvironmentValues {
    /// The selection namespace of the enclosing `DSTabBarContainer`. Nil elsewhere.
    var dsTabBarSelectionNamespace: Namespace.ID? {
        get { self[DSTabBarSelectionNamespaceKey.self] }
        set { self[DSTabBarSelectionNamespaceKey.self] = newValue }
    }
}

/// The selection pill's fill and matched geometry id, shared by the bar slot and the shelf.
enum DSTabBarSelectionPill {
    static let matchedGeometryId = "selection"

    static func fill(_ shape: some Shape) -> some View {
        shape.fill(Color.bgSurfaceContainerInverted.opacity(DSTabBarStyle.selectionFillOpacity))
    }
}

private struct DSTabBarSelectionNamespaceKey: EnvironmentKey {
    static let defaultValue: Namespace.ID? = nil
}

private struct DSTabBarSelectionPillModifier: ViewModifier {
    let isSelected: Bool
    @Environment(\.dsTabBarSelectionNamespace) private var namespace

    func body(content: Content) -> some View {
        content.background {
            if isSelected, let namespace {
                DSTabBarSelectionPill.fill(
                    RoundedRectangle(cornerRadius: DSTabBarStyle.shelfSelectionCornerRadius, style: .continuous)
                )
                .matchedGeometryEffect(id: DSTabBarSelectionPill.matchedGeometryId, in: namespace)
            }
        }
    }
}
