import Foundation
import SubstrateSdk
import CommonService

enum HydraSubstrateBalanceMapping {
    static func getMappingKey(for accountAsset: HydraAccountAsset) -> String {
        [
            "balance",
            accountAsset.accountId.toHex(),
            String(accountAsset.assetId)
        ].joined(separator: ":")
    }

    static func getBalance(
        for accountAsset: HydraAccountAsset,
        store: BatchDictSubscriptionState
    ) throws -> HydraBalance {
        let mappingKey = getMappingKey(for: accountAsset)

        if accountAsset.assetId == HydraDx.nativeAssetId {
            let account: SystemPallet.AccountInfo? = try store.decode(for: mappingKey)

            return HydraBalance(accountInfo: account)
        } else {
            let account: OrmlAccount? = try store.decode(for: mappingKey)

            return HydraBalance(ormlAccount: account)
        }
    }
}
