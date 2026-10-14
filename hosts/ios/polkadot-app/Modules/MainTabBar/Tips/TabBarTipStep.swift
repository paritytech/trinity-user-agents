import TipKit

struct TabBarTipStep {
    let tip: AnyTip
    let anchor: TabBarTipAnchor

    init(tip: some Tip, anchor: TabBarTipAnchor) {
        self.tip = AnyTip(tip)
        self.anchor = anchor
    }
}
