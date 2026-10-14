import Foundation

struct NavigationBarSettings {
    let style: NavigationBarStyle
    let shouldSetCloseButton: Bool
}

extension NavigationBarSettings {
    func bySettingCloseButton(_ value: Bool) -> NavigationBarSettings {
        NavigationBarSettings(style: style, shouldSetCloseButton: value)
    }
}
