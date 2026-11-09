import Foundation
import DesignSystem

extension NavigationBarSettings {
    static var defaultSettings: NavigationBarSettings {
        NavigationBarSettings(style: .defaultStyle, shouldSetCloseButton: true)
    }

    static var transparentSettings: NavigationBarSettings {
        NavigationBarSettings(style: .transparentStyle, shouldSetCloseButton: true)
    }

    static var shadowSettings: NavigationBarSettings {
        NavigationBarSettings(style: .shadowStyle, shouldSetCloseButton: true)
    }

    /// A bare bar over the camera preview, with a white close button.
    static var scannerSettings: NavigationBarSettings {
        NavigationBarSettings(
            style: NavigationBarStyle(
                backgroundColor: nil,
                shadow: nil,
                shadowColor: nil,
                tintColor: .fgStaticWhite,
                backImage: nil,
                backgroundEffect: nil,
                titleAttributes: nil,
                largeTitleAttributes: nil
            ),
            shouldSetCloseButton: true
        )
    }
}
