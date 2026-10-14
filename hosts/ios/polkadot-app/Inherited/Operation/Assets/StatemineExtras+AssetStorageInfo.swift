import Foundation
import ChainRegistry

extension StatemineAssetExtras {
    init(info: AssetsPalletStorageInfo) {
        self.init(assetId: info.assetIdString, palletName: info.palletName)
    }
}
