import Foundation
import UIKit
import Products

@MainActor
protocol SPABrowserCoordinating: AnyObject {
    var tabs: [SPATab] { get }

    func addObserver(_ observer: SPATabsObserver, sendOnSubscription: Bool)
    func findOrCreateTab(for page: ProductPage) -> SPATab
    func controller(for tab: SPATab) -> UIViewController?
    func close(tabId: UUID)
}

@MainActor
final class SPABrowserCoordinator {
    private let tabManager: SPATabManaging
    private let pool: SPAControllerPooling

    init(
        tabManager: SPATabManaging,
        pool: SPAControllerPooling
    ) {
        self.tabManager = tabManager
        self.pool = pool
    }
}

// MARK: - SPABrowserCoordinating

extension SPABrowserCoordinator: SPABrowserCoordinating {
    var tabs: [SPATab] { tabManager.getAllTabs() }

    func addObserver(_ observer: SPATabsObserver, sendOnSubscription: Bool) {
        tabManager.addObserver(observer, sendOnSubscription: sendOnSubscription)
    }

    func findOrCreateTab(for page: ProductPage) -> SPATab {
        let dotDomain = page.host.toDotDomain()

        guard var existing = tabManager.getAllTabs().first(where: { $0.dotDomain == dotDomain }) else {
            let tab = SPATab(dotDomain: dotDomain, page: page.page)
            tabManager.updateTab(tab)

            return tab
        }

        // A request without a page means the product root; the tab may have wandered off it since
        // the last request, so it is always navigated even when the recorded page is already nil.
        let pageChanged = page.page != existing.page

        if pageChanged {
            existing.page = page.page
            tabManager.updateTab(existing)
        }

        if pageChanged || page.page == nil {
            pool.controller(for: existing.id)?.navigate(to: page)
        }

        return existing
    }

    func controller(for tab: SPATab) -> UIViewController? {
        pool.makeController(for: tab)?.controller
    }

    func close(tabId: UUID) {
        pool.removeController(for: tabId)
        tabManager.removeTab(with: tabId)
    }
}
