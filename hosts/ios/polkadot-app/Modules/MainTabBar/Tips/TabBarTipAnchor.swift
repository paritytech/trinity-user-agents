/// Where a tip's popover points. The center button anchor is a layout guide the chrome places over the
/// resting button; the chain-status strip is a hosting-controller view the chrome does not own, so it arrives
/// as a closure.
enum TabBarTipAnchor {
    case centerButton
    case statusStrip
}
