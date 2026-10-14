/// Where a tip's popover points. Bar items resolve through the slot map; the chain-status
/// strip is a hosting-controller view the chrome does not own, so it arrives as a closure.
enum TabBarTipAnchor {
    case barItem(TabBarSlot)
    case statusStrip
}
