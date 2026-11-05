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

/// Shown beside the Edge glow the first time the user tucks the Docklet. Not a `TabBarTip`: its rules need the bar
/// shown, and this tip exists to be seen while the bar is tucked.
struct DockletTuckedTip: Tip {
    var title: Text { Text(.Tips.tuckedTitle) }
    var message: Text? { Text(.Tips.tuckedMessage) }

    var rules: [Rule] {
        [#Rule(TabBarTips.$isUserTuckLanded) { $0 == true }]
    }

    var options: [any Option] { [MaxDisplayCount(1)] }
}

/// Shown beside the Edge glow the first time the Docklet tucks itself.
struct DockletAutoHiddenTip: Tip {
    var title: Text { Text(.Tips.autoHiddenTitle) }
    var message: Text? { Text(.Tips.autoHiddenMessage) }

    var rules: [Rule] {
        [#Rule(TabBarTips.$isAutoHideLanded) { $0 == true }]
    }

    var options: [any Option] { [MaxDisplayCount(1)] }
}
