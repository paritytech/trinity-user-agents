import Foundation
import SubstrateSdk

extension HydraDx {
    static var dynamicFeesPath: StorageCodingPath {
        StorageCodingPath(moduleName: dynamicFeesModule, itemName: "AssetFee")
    }

    static var feeCurrenciesPath: StorageCodingPath {
        StorageCodingPath(moduleName: multiTxPaymentModule, itemName: "AcceptedCurrencies")
    }

    static var accountFeeCurrencyPath: StorageCodingPath {
        StorageCodingPath(moduleName: multiTxPaymentModule, itemName: "AccountCurrencyMap")
    }

    static var referralLinkedAccountPath: StorageCodingPath {
        StorageCodingPath(moduleName: referralsModule, itemName: "LinkedAccounts")
    }
}
