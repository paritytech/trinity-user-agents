import TipKit

struct TabBarTipStep {
    let tip: AnyTip
    let anchor: TabBarTipAnchor
    let onShow: (@MainActor () -> Void)?

    init(
        tip: some Tip,
        anchor: TabBarTipAnchor,
        onShow: (@MainActor () -> Void)? = nil
    ) {
        self.tip = AnyTip(tip)
        self.anchor = anchor
        self.onShow = onShow
    }
}
