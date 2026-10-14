import Foundation

@MainActor
final class SPATabManager: SPATabManaging {
    private var tabsById: [UUID: SPATab] = [:]
    private var observers: [WeakObserverBox] = []

    private struct WeakObserverBox { weak var observer: SPATabsObserver? }

    func addObserver(_ observer: SPATabsObserver, sendOnSubscription: Bool) {
        observers = observers.filter { $0.observer != nil }
        observers.append(WeakObserverBox(observer: observer))
        if sendOnSubscription { observer.didReceiveUpdatedTabs(sortedTabs()) }
    }

    func getAllTabs() -> [SPATab] { sortedTabs() }

    func updateTab(_ tab: SPATab) {
        tabsById[tab.id] = tab
        notify()
    }

    func removeTab(with id: UUID) {
        tabsById[id] = nil
        notify()
    }

    private func sortedTabs() -> [SPATab] {
        tabsById.values.sorted { $0.createdAt < $1.createdAt }
    }

    private func notify() {
        let tabs = sortedTabs()
        observers = observers.filter { $0.observer != nil }
        observers.forEach { $0.observer?.didReceiveUpdatedTabs(tabs) }
    }
}
