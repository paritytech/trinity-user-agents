import Foundation

/// Where the selection pill sits. It marks what is on screen: the current tab, the Apps item while a product
/// is mounted, or that product's cell while the shelf is open.
enum TabBarSelectionPlacement: Equatable {
    case tab(itemIndex: Int)
    case appsItem
    case productCell(UUID)
}

enum TabBarSelectionPolicy {
    static func placement(
        tabItemIndex: Int,
        mountedProductId: UUID?,
        isShelfOpen: Bool
    ) -> TabBarSelectionPlacement {
        guard let mountedProductId else {
            return .tab(itemIndex: tabItemIndex)
        }
        return isShelfOpen ? .productCell(mountedProductId) : .appsItem
    }
}
