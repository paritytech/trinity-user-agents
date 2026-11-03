import UIKit

/// A screen adopts this to control tab bar chrome visibility. Returning `false` from
/// `tabBarChromeIsVisible` hides the chrome entirely (no peek, no grab zone),
/// signalling that tab bar navigation is disallowed on this screen.
protocol TabBarChromeVisibilityCustomizing {
    var tabBarChromeIsVisible: Bool { get }
}

/// A screen pushed over a tab's root that the tab bar treats as the root itself: the bar stays shown, cannot fold,
/// keeps the root's clearance, and ignores a tap on the selected tab.
protocol TabBarRootStandIn {}
