import DesignSystem
import SwiftUI

public extension DSTabBarStyle {
    /// Tab bar colours that are not a plain theme token: the brief's token-plus-opacity values, strokeGlass (local
    /// until the design system ships it) and the glass fill.
    enum Palette {
        /// The glass outline; the brief's strokeGlass token, local until the design system ships it.
        public static let strokeGlass = Color.bgSurfaceContainerInverted.opacity(0.12)
        /// The selection pill's flat fill.
        public static let selectionFill = Color.bgSurfaceContainerInverted.opacity(0.12)
        /// The grabber along the top of the open centre panel.
        public static let grabber = Color.bgSurfaceContainerInverted.opacity(0.32)

        /// The glass fill over the frost before iOS 26: the mockup's bg/surface/container at 28%.
        public static let glassFill = Color.bgSurfaceContainer.opacity(0.28)
        /// The fill the centre control settles to as its panel opens.
        public static let centerOpenFill = Color(hex: 0x080808).opacity(0.3)
        /// The scrim behind the bar while the centre panel is open.
        public static let dimWash = Color.black.opacity(0.3)
    }
}
