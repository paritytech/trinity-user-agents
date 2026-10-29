import Foundation
import Testing
@testable import polkadot_app

@Suite("TabBarSelectionPolicy")
struct TabBarSelectionPolicyTests {
    @Test("The pill marks the current tab when no product is mounted")
    func marksTabWithoutMountedProduct() {
        let placement = TabBarSelectionPolicy.placement(
            tabItemIndex: 1,
            mountedProductId: nil,
            isShelfOpen: false
        )
        #expect(placement == .tab(itemIndex: 1))
    }

    @Test("Opening the shelf alone leaves the pill on the current tab")
    func shelfOpenWithoutMountedProductKeepsTab() {
        let placement = TabBarSelectionPolicy.placement(
            tabItemIndex: 1,
            mountedProductId: nil,
            isShelfOpen: true
        )
        #expect(placement == .tab(itemIndex: 1))
    }

    @Test("The pill marks the Apps item while a product is on screen")
    func marksAppsItemWithMountedProductAndShelfClosed() {
        let placement = TabBarSelectionPolicy.placement(
            tabItemIndex: 1,
            mountedProductId: UUID(),
            isShelfOpen: false
        )
        #expect(placement == .appsItem)
    }

    @Test("The pill moves to the product's cell while the shelf is open")
    func marksProductCellWithMountedProductAndShelfOpen() {
        let productId = UUID()
        let placement = TabBarSelectionPolicy.placement(
            tabItemIndex: 1,
            mountedProductId: productId,
            isShelfOpen: true
        )
        #expect(placement == .productCell(productId))
    }
}
