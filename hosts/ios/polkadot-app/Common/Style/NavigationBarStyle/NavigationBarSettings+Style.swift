import Foundation

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
}
