import Foundation
import Operation_iOS
import SubstrateSdk
import OperationExt

typealias RecentContactsProtocol = AnyProviderAutoCleaning & RecentContactsStorageSubscriber &
    RecentContactsSubscriptionHandler

protocol RecentContactsManaging: RecentContactsProtocol {
    func setup(_ delegate: RecentContactsServiceDelegate?, chainAssetID: ChainAssetId?)
}

extension RecentContactsManaging {
    func setup(_ delegate: RecentContactsServiceDelegate?) {
        setup(delegate, chainAssetID: nil)
    }
}
