import TipKit

/// What a tap on the tip's body does. A tap always invalidates the tip; `.restoreDocklet` also brings the Docklet back.
enum TabBarTipTap {
    case dismiss
    case restoreDocklet
}

struct TabBarTipStep {
    let tip: AnyTip
    let anchor: TabBarTipAnchor
    let tap: TabBarTipTap
    let onShow: (@MainActor () -> Void)?

    init(
        tip: some Tip,
        anchor: TabBarTipAnchor,
        tap: TabBarTipTap = .dismiss,
        onShow: (@MainActor () -> Void)? = nil
    ) {
        self.tip = AnyTip(tip)
        self.anchor = anchor
        self.tap = tap
        self.onShow = onShow
    }
}
