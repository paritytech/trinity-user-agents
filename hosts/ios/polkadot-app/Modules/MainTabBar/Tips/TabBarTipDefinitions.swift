import SwiftUI
import TipKit

/// Every tab bar tip waits for the bar to be on screen at a tab root and shows once.
/// Each stays a distinct type: TipKit keys display state and eligibility by type.
protocol TabBarTip: Tip {}

extension TabBarTip {
    var rules: [Rule] {
        [
            #Rule(TabBarTips.$isBarShownAtRoot) { $0 == true },
            #Rule(TabBarTips.$isBarLaidOut) { $0 == true }
        ]
    }

    var options: [any Option] { [MaxDisplayCount(1)] }
}

struct ChainStatusStripTip: TabBarTip {
    var title: Text { Text(.Tips.chainStatusTitle) }
    var message: Text? { Text(.Tips.chainStatusMessage) }
}

struct ScanActionTip: TabBarTip {
    var title: Text { Text(.Tips.scanTitle) }
    var message: Text? { Text(.Tips.scanMessage) }
}
