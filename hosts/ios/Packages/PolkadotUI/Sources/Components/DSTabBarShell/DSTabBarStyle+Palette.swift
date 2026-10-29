import DesignSystem
import SwiftUI

public extension DSTabBarStyle {
    /// Tab bar colours that are not a plain theme token: the brief's token-plus-opacity values, strokeGlass (local
    /// until the design system ships it) and the dark-theme glass wash.
    enum Palette {
        /// The glass outline; the brief's strokeGlass token, local until the design system ships it.
        public static let strokeGlass = Color.bgSurfaceContainerInverted.opacity(0.12)
        /// The selection pill's flat fill.
        public static let selectionFill = Color.bgSurfaceContainerInverted.opacity(0.12)
        /// The grabber along the top of the open centre panel.
        public static let grabber = Color.bgSurfaceContainerInverted.opacity(0.32)

        /// The light end of the glass wash: the top centre of the bar and the top of the centre control.
        public static let glassLight = Color(hex: 0x919191).opacity(0.2)
        /// The dark end of the glass wash: the edge of the bar and the foot of the centre control.
        public static let glassDark = Color(hex: 0x2B2B2B).opacity(0.2)
        /// The fill the centre control settles to as its panel opens.
        public static let centerOpenFill = Color(hex: 0x080808).opacity(0.3)
        /// The scrim behind the bar while the centre panel is open.
        public static let dimWash = Color.black.opacity(0.3)
    }
}
