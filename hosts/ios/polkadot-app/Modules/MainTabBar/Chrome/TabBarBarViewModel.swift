import Observation
import PolkadotUI

/// What the SwiftUI bar row shows. The chrome writes it; the bar row reports slot taps back through `onSlotTapped`.
@MainActor
@Observable
final class TabBarBarViewModel {
    var items: [DSTabBarItem] = []
    var selectedItemIndex = 0
    var activeActionItemIndex: Int?
    var onSlotTapped: ((Int) -> Void)?
}
