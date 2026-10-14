import Foundation
import Operation_iOS
import SubstrateSdk

protocol RecentContactsStorageSubscriber: LocalStorageProviderObserving where Self: AnyObject {
    var recentContactsSubscriptionFactory: RecentContactsSubscriptionFactoryProtocol { get }

    var recentContactsSubscriptionHandler: RecentContactsSubscriptionHandler { get }

    func subscribeAllRecentContacts(for chainAssetID: ChainAssetId) -> StreamableProvider<RecentContactModel>?
    func subscribeAllRecentContacts() -> StreamableProvider<RecentContactModel>?
}

extension RecentContactsStorageSubscriber {
    func subscribeAllRecentContacts() -> StreamableProvider<RecentContactModel>? {
        guard let provider = try? recentContactsSubscriptionFactory.getAllRecentContacts() else {
            return nil
        }

        addStreamableProviderObserver(
            for: provider,
            updateClosure: { [weak self] changedItems in
                self?.recentContactsSubscriptionHandler
                    .handleAllRecentContacts(result: .success(changedItems))
            },
            failureClosure: { [weak self] error in
                self?.recentContactsSubscriptionHandler.handleAllRecentContacts(result: .failure(error))
            },
            options: .allNonblocking()
        )

        return provider
    }

    func subscribeAllRecentContacts(for chainAssetID: ChainAssetId) -> StreamableProvider<RecentContactModel>? {
        guard let provider = try? recentContactsSubscriptionFactory.getAllRecentContacts(for: chainAssetID) else {
            return nil
        }

        addStreamableProviderObserver(
            for: provider,
            updateClosure: { [weak self] changedItems in
                self?.recentContactsSubscriptionHandler.handleAllRecentContacts(
                    for: chainAssetID,
                    result: .success(changedItems)
                )
            },
            failureClosure: { [weak self] error in
                self?.recentContactsSubscriptionHandler.handleAllRecentContacts(
                    for: chainAssetID,
                    result: .failure(error)
                )
            },
            options: .allNonblocking()
        )

        return provider
    }
}

extension RecentContactsStorageSubscriber where Self: RecentContactsSubscriptionHandler {
    var recentContactsSubscriptionHandler: RecentContactsSubscriptionHandler { self }
}
