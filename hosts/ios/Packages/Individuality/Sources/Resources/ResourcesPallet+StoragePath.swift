import Foundation
import SubstrateStorageSubscription
import SubstrateSdk
import SubstrateSdkExt

public extension ResourcesPallet {
    static var consumers: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "Consumers")
    }

    static var usernameOwnerOf: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "UsernameOwnerOf")
    }

    static var usernameReservationQueue: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "UsernameReservationQueue")
    }

    static var usernameReservationDuration: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "UsernameReservationDuration")
    }

    static var statementStoreAllowances: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "StatementStoreAllowances")
    }

    static var notificationRegistrationByAlias: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "NotificationRegistrationByAlias")
    }

    static var spentLongTermStorageAliases: StorageCodingPath {
        StorageCodingPath(moduleName: name, itemName: "SpentLongTermStorageAliases")
    }
}

public extension ResourcesPallet {
    enum Storage {
        case consumers(AccountId)
    }
}

extension ResourcesPallet.Storage: StoragePathConvertible {
    public var moduleName: String {
        ResourcesPallet.name
    }

    public var name: String {
        switch self {
        case .consumers:
            "Consumers"
        }
    }
}

extension ResourcesPallet.Storage: SubscriptionRequestConvertible {
    public var request: any SubscriptionRequestProtocol {
        switch self {
        case let .consumers(accountId):
            MapSubscriptionRequest(
                storagePath: self(),
                localKey: "",
                keyParamClosure: { BytesCodable(wrappedValue: accountId) }
            )
        }
    }
}
