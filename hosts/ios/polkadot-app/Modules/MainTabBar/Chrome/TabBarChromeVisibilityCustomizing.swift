import UIKit

/// A screen adopts this to control tab bar chrome visibility. Returning `false` from
/// `tabBarChromeIsVisible` hides the chrome entirely (no peek, no grab zone),
/// signalling that tab bar navigation is disallowed on this screen.
protocol TabBarChromeVisibilityCustomizing {
    var tabBarChromeIsVisible: Bool { get }
}
