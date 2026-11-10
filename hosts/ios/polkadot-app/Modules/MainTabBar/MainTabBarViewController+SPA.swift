import struct TrUAPIHost.ProductId
import UIKit
import PolkadotUI
import UIKitExt
import Products

// MARK: - SPAHosting

extension MainTabBarViewController: SPAHosting {
    func openProduct(page: ProductPage) {
        #if FEATURE_PRODUCTS
            // What is presented over the container would hide what it mounts.
            presentedViewController?.dismiss(animated: true)
            let tab = browserCoordinator.findOrCreateTab(for: page)
            mountSPA(for: tab)
        #else
            if let presented = presentedProduct(page.host.toDotDomain()) {
                uncover(presented)
                presented.navigate(to: page)
                return
            }

            guard let current = firstPresentedProduct(where: { $0.configuration.presentation == .fullScreen }) else {
                presentFullScreenProduct(page: page)
                return
            }

            confirmReplacing(current, with: page)
        #endif
    }

    /// Presents a product as a one-shot sheet over the container. It never enters the tab store,
    /// so closing it leaves no shelf chip.
    func presentProduct(page: ProductPage) {
        guard let view = SPAViewFactory.createView(
            page: page,
            flowState: flowStateProvider.flowState(),
            presentation: .sheet
        ) else {
            return
        }

        view.controller.sheetPresentationController?.detents = [
            .custom { $0.maximumDetentValue * 0.92 }
        ]
        view.controller.sheetPresentationController?.prefersGrabberVisible = true

        let host = UIWindow.keyWindow?.topmostViewController ?? self
        host.present(view.controller, animated: true)
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
        (controller as? StatusBarTitleProviding)?.onStatusBarTitleChanged = { [weak self] in
            self?.refreshStatusBarTitle()
        }
        container.mountSPA(controller, for: tab.id)
        applyChrome(.spa(controller))
        applyChips()
    }

    func applyChips() {
        let chips = spaChipViewModels.map {
            TabBarShelfChip(id: $0.id, name: $0.name, icon: $0.icon)
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
        chromeController.setPanel(nil)
        mountSPA(for: tab)
        return true
    }

    /// The first product presented over the container, from the bottom of the presentation chain, that matches.
    func firstPresentedProduct(
        where predicate: (SPASimplifiedViewController) -> Bool
    ) -> SPASimplifiedViewController? {
        var controller = presentedViewController

        while let current = controller {
            if let product = current as? SPASimplifiedViewController, predicate(product) {
                return product
            }
            controller = current.presentedViewController
        }

        return nil
    }

    /// The product presented over the container, so a repeated open reuses it instead of stacking a copy.
    func presentedProduct(_ productId: ProductId) -> SPASimplifiedViewController? {
        firstPresentedProduct { $0.configuration.productDomain == productId }
    }

    /// Brings the presented product to the front by dismissing whatever covers it.
    /// Returns false when nothing covered it.
    @discardableResult
    func uncover(_ product: SPASimplifiedViewController) -> Bool {
        guard product.presentedViewController != nil else {
            return false
        }

        product.dismiss(animated: true)
        return true
    }
}

// MARK: - Private

private extension MainTabBarViewController {
    var mountedSPATabId: UUID? {
        guard case let .spa(id) = container.selection else {
            return nil
        }
        return id
    }

    func presentFullScreenProduct(page: ProductPage) {
        guard let view = SPAViewFactory.createView(
            page: page,
            flowState: flowStateProvider.flowState(),
            presentation: .fullScreen
        ) else {
            return
        }

        let host = UIWindow.keyWindow?.topmostViewController ?? self
        host.present(view.controller, animated: true)
    }

    /// One full-screen product at a time: opening another one replaces it once the user agrees.
    func confirmReplacing(_ current: SPASimplifiedViewController, with page: ProductPage) {
        let currentName = current.navigationItem.title ?? current.configuration.page.host.name
        let alert = UIAlertController(
            title: String(localized: .Products.productReplaceTitle(name: page.host.name)),
            message: String(localized: .Products.productReplaceMessage(name: currentName)),
            preferredStyle: .alert
        )
        alert.addAction(UIAlertAction(title: String(localized: .Common.cancel), style: .cancel))
        alert.addAction(UIAlertAction(
            title: String(localized: .Products.productReplaceOpen),
            style: .default
        ) { [weak self, weak current] _ in
            self?.replace(current, with: page)
        })

        let host = UIWindow.keyWindow?.topmostViewController ?? self
        host.present(alert, animated: true)
    }

    func replace(_ current: SPASimplifiedViewController?, with page: ProductPage) {
        guard let presenting = current?.presentingViewController else {
            presentFullScreenProduct(page: page)
            return
        }

        presenting.dismiss(animated: true) { [weak self] in
            self?.presentFullScreenProduct(page: page)
        }
    }
}
