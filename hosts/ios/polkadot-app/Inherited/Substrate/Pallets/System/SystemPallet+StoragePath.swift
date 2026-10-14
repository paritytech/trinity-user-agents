import Foundation
import SubstrateSdk
import SubstrateStorageSubscription
import SubstrateSdkExt

extension SystemPallet {
    enum Storage {
        case account(AccountId)
    }
}

extension SystemPallet.Storage: StoragePathConvertible {
    var moduleName: String {
        SystemPallet.name
    }

    var name: String {
        switch self {
        case .account:
            "Account"
        }
    }
}

extension SystemPallet.Storage: SubscriptionRequestConvertible {
    var request: any SubscriptionRequestProtocol {
        switch self {
        case let .account(accountId):
            MapSubscriptionRequest(
                storagePath: self(),
                localKey: "",
                keyParamClosure: { BytesCodable(wrappedValue: accountId) }
            )
        }
    }
}
