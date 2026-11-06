import Foundation

/// Where the selection pill sits. It marks what is on screen: the current tab, the Apps item while a product
/// is mounted, or that product's cell while the shelf is open.
enum TabBarSelectionPlacement: Equatable {
    case tab(itemIndex: Int)
    case appsItem
    case productCell(UUID)
}

extension TabBarSelectionPlacement {
    /// The placement for the current tab, the mounted product's Apps item, or its shelf cell while the shelf is open.
    init(tabItemIndex: Int, mountedProductId: UUID?, isShelfOpen: Bool) {
        guard let mountedProductId else {
            self = .tab(itemIndex: tabItemIndex)
            return
        }
        self = isShelfOpen ? .productCell(mountedProductId) : .appsItem
    }
}
