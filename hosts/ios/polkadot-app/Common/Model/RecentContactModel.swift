import Foundation
import Operation_iOS
import SubstrateSdk

struct RecentContactModel: Identifiable, Hashable {
    var identifier: String {
        "\(chainAssetID.assetId) - \(chainAssetID.chainId) - \(accountID.toHex())"
    }

    let accountID: AccountId
    let lastUsed: Date
    let chainAssetID: ChainAssetId

    init(
        accountID: AccountId,
        lastUsed: Date = Date(),
        chainAssetID: ChainAssetId
    ) {
        self.accountID = accountID
        self.lastUsed = lastUsed
        self.chainAssetID = chainAssetID
    }
}
