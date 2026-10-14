/// Absorbs the skew between bar item indices and tab indices: actions occupy item slots but are
/// absent from the tab list the container selects against.
struct TabBarSlotMap: Equatable {
    private let slots: [TabBarSlot]
    private let tabItemIndices: [Int]

    init(slots: [TabBarSlot]) {
        self.slots = slots
        tabItemIndices = slots.indices.filter { slots[$0].tab != nil }
    }

    func itemIndex(forTabIndex tabIndex: Int) -> Int? {
        guard tabItemIndices.indices.contains(tabIndex) else {
            return nil
        }
        return tabItemIndices[tabIndex]
    }

    func tabIndex(forItemIndex itemIndex: Int) -> Int? {
        tabItemIndices.firstIndex(of: itemIndex)
    }

    func action(forItemIndex itemIndex: Int) -> TabBarAction? {
        guard slots.indices.contains(itemIndex), case let .action(action) = slots[itemIndex] else {
            return nil
        }
        return action
    }

    func itemIndex(for slot: TabBarSlot) -> Int? {
        slots.firstIndex(of: slot)
    }

    func itemIndex(for action: TabBarAction) -> Int? {
        itemIndex(for: .action(action))
    }
}
