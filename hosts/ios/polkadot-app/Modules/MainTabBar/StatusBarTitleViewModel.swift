import DesignSystem
import UIKit

struct StatusBarTitleViewModel {
    let title: String
    let showsLogo: Bool
    let menu: UIMenu?
}

@MainActor
protocol StatusBarTitleProviding: AnyObject {
    var statusBarTitle: String? { get }
    var onStatusBarTitleChanged: (() -> Void)? { get set }

    func makeStatusBarMenu() -> UIMenu
}

/// A screen that draws its own content under the status strip in a theme other than the app's.
@MainActor
protocol StatusStripThemeProviding: AnyObject {
    /// The theme the status strip takes over this screen, or nil for the app's theme.
    var statusStripTheme: ThemeSelection? { get }
    var onStatusStripThemeChanged: (() -> Void)? { get set }
}
