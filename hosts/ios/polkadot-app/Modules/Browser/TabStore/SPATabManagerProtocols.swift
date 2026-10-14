import Foundation

@MainActor
protocol SPATabsObserver: AnyObject {
    func didReceiveUpdatedTabs(_ tabs: [SPATab])
}

@MainActor
protocol SPATabManaging: AnyObject {
    func addObserver(_ observer: SPATabsObserver, sendOnSubscription: Bool)
    func getAllTabs() -> [SPATab]
    func updateTab(_ tab: SPATab)
    func removeTab(with id: UUID)
}
