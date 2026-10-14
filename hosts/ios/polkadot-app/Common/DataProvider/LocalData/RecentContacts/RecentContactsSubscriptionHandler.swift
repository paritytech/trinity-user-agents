import Foundation
import Operation_iOS
import SubstrateSdk

protocol RecentContactsSubscriptionHandler {
    func handleAllRecentContacts(result: Result<[DataProviderChange<RecentContactModel>], Error>)
    func handleAllRecentContacts(
        for chainAssetID: ChainAssetId,
        result: Result<[DataProviderChange<RecentContactModel>], Error>
    )
}

extension RecentContactsSubscriptionHandler {
    func handleAllRecentContacts(result _: Result<[DataProviderChange<RecentContactModel>], Error>) {}

    func handleAllRecentContacts(
        for _: ChainAssetId,
        result _: Result<[DataProviderChange<RecentContactModel>], Error>
    ) {}
}
