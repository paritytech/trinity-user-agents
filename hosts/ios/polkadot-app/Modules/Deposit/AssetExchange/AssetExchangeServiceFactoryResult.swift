import Foundation
import SubstrateSdk
import AssetExchange
import KeyDerivation

struct AssetExchangeServiceFactoryResult {
    let service: AssetsExchangeServiceProtocol
    let fundedAssetId: ChainAssetId
    let walletToDeposit: WalletManaging
    let accountToFund: AccountId
}
