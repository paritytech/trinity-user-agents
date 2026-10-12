import UIKit
import PolkadotUI
import UIKitExt
import Products

// MARK: - SPAHosting

extension MainTabBarViewController: SPAHosting {
    func openProduct(page: ProductPage) {
        // A card is a modal over the container, so what it links to mounts unseen.
        presentedViewController?.dismiss(animated: true)
        #if FEATURE_PRODUCTS
            let tab = browserCoordinator.findOrCreateTab(for: page)
            mountSPA(for: tab)
        #else
            presentProduct(page: page)
        #endif
    }

    func minimizeSPA() {
        container.unmountSPA()
        refreshChrome()
    }

    func closeSPA(tabId: UUID) {
        let wasMounted = container.selection == .spa(tabId)
        browserCoordinator.close(tabId: tabId)

        guard wasMounted else {
            return
        }
        minimizeSPA()
    }
}

extension MainTabBarViewController {
    static let productGamePillID = AppWidgetID("productGame")

    func mountSPA(for tab: SPATab) {
        guard let controller = browserCoordinator.controller(for: tab) else {
            closeSPA(tabId: tab.id)
            return
        }
        container.mountSPA(controller, for: tab.id)
        chromeController.apply(.spa(controller))
        applyChips()
    }

    func applyChips() {
        let chips = spaChipViewModels.map {
            DSTabBarChip(id: $0.id, name: $0.name, icon: $0.icon)
        }
        chromeController.setSPATabs(chips, selected: mountedSPATabId)
        applyProductGamePill()
    }

    /// The countdown stays off the screen of the product it counts down to.
    func applyProductGamePill() {
        guard let pill = productGamePill, pill.productId != mountedProductId else {
            detachWidget(for: Self.productGamePillID)
            return
        }
        attachWidget(pill.configuration, for: Self.productGamePillID)
    }

    var mountedProductId: ProductId? {
        mountedSPATabId.flatMap { id in browserCoordinator.tabs.first { $0.id == id }?.dotDomain }
    }

    func mountExistingTab(where predicate: (SPATab) -> Bool) -> Bool {
        guard let tab = browserCoordinator.tabs.first(where: predicate) else {
            return false
        }
        chromeController.setPanel(nil, animated: true)
        mountSPA(for: tab)
        return true
    }
}

// MARK: - Private

private extension MainTabBarViewController {
    #if !FEATURE_PRODUCTS
        /// Without the browse tab a product is a one-shot rather than a peer of the tabs: it is
        /// presented over the container, so it never enters the tab store and closing it leaves
        /// no chip behind.
        func presentProduct(page: ProductPage) {
            guard let view = SPAViewFactory.createView(
                page: page,
                flowState: flowStateProvider.flowState(),
                isBrowserTab: true
            ) else {
                return
            }

            view.controller.modalPresentationStyle = .pageSheet
            view.controller.sheetPresentationController?.detents = [
                .custom { $0.maximumDetentValue * 0.92 }
            ]
            view.controller.sheetPresentationController?.prefersGrabberVisible = true

            let host = UIWindow.keyWindow?.topmostViewController ?? self
            host.present(view.controller, animated: true)
        }
    #endif

    var mountedSPATabId: UUID? {
        guard case let .spa(id) = container.selection else {
            return nil
        }
        return id
    }
}
