import Foundation
import SubstrateStorageSubscription
import SubstrateSdk
import SubstrateSdkExt

public extension DotnsGatewayPallet {
    /// Storage maps of the dotNS gateway pallet, keyed by their lookup parameters.
    enum Storage {
        /// `AccountNames`, keyed by account id.
        case accountNames(AccountId)
    }
}

extension DotnsGatewayPallet.Storage: StoragePathConvertible {
    public var moduleName: String {
        DotnsGatewayPallet.name
    }

    public var name: String {
        switch self {
        case .accountNames:
            "AccountNames"
        }
    }
}

extension DotnsGatewayPallet.Storage: SubscriptionRequestConvertible {
    public var request: any SubscriptionRequestProtocol {
        switch self {
        case let .accountNames(accountId):
            MapSubscriptionRequest(
                storagePath: self(),
                localKey: "",
                keyParamClosure: { BytesCodable(wrappedValue: accountId) }
            )
        }
    }
}
