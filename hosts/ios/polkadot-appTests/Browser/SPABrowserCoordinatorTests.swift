import Foundation
import Testing
import UIKit
import UIKitExt
@testable import Products
@testable import polkadot_app

@MainActor
struct SPABrowserCoordinatorTests {
    private let host = ProductHost.parse("browse.dot", tld: "dot")!

    private func makeSUT(existing: SPATab?) -> (SPABrowserCoordinator, StubSPATabManager, StubSPAView) {
        let tabManager = StubSPATabManager(tabs: existing.map { [$0] } ?? [])
        let view = StubSPAView()
        let pool = StubSPAControllerPool(view: view)
        return (SPABrowserCoordinator(tabManager: tabManager, pool: pool), tabManager, view)
    }

    @Test("a nil page on a tab that is on a deep page navigates back to the root")
    func nilPageLeavesDeepPage() {
        let tab = SPATab(dotDomain: "browse.dot", page: "/onboarding")
        let (sut, tabManager, view) = makeSUT(existing: tab)

        let result = sut.findOrCreateTab(for: ProductPage(host: host, page: nil))

        #expect(result.id == tab.id)
        #expect(tabManager.tabs.first?.page == nil)
        #expect(view.navigatedPages.map(\.page) == [nil])
    }

    @Test("a nil page on a tab already recorded at the root still navigates to the root")
    func nilPageOnRootTabNavigates() {
        let tab = SPATab(dotDomain: "browse.dot", page: nil)
        let (sut, _, view) = makeSUT(existing: tab)

        _ = sut.findOrCreateTab(for: ProductPage(host: host, page: nil))

        #expect(view.navigatedPages.count == 1)
        #expect(view.navigatedPages.first?.page == nil)
    }

    @Test("requesting the page a tab is already on does not navigate")
    func samePageDoesNotNavigate() {
        let tab = SPATab(dotDomain: "browse.dot", page: "/onboarding")
        let (sut, _, view) = makeSUT(existing: tab)

        _ = sut.findOrCreateTab(for: ProductPage(host: host, page: "/onboarding"))

        #expect(view.navigatedPages.isEmpty)
    }

    @Test("an unknown domain creates a tab without navigating")
    func newTabDoesNotNavigate() {
        let (sut, tabManager, view) = makeSUT(existing: nil)

        let result = sut.findOrCreateTab(for: ProductPage(host: host, page: "/onboarding"))

        #expect(tabManager.tabs == [result])
        #expect(result.page == "/onboarding")
        #expect(view.navigatedPages.isEmpty)
    }
}

@MainActor
private final class StubSPATabManager: SPATabManaging {
    private(set) var tabs: [SPATab]

    init(tabs: [SPATab]) {
        self.tabs = tabs
    }

    func addObserver(_: SPATabsObserver, sendOnSubscription _: Bool) {}
    func getAllTabs() -> [SPATab] { tabs }

    func updateTab(_ tab: SPATab) {
        if let index = tabs.firstIndex(where: { $0.id == tab.id }) {
            tabs[index] = tab
        } else {
            tabs.append(tab)
        }
    }

    func removeTab(with id: UUID) {
        tabs.removeAll { $0.id == id }
    }
}

@MainActor
private final class StubSPAControllerPool: SPAControllerPooling {
    private let view: StubSPAView

    init(view: StubSPAView) {
        self.view = view
    }

    func controller(for _: UUID) -> SPAViewProtocol? { view }
    func makeController(for _: SPATab) -> SPAViewProtocol? { view }
    func removeController(for _: UUID) {}
}
