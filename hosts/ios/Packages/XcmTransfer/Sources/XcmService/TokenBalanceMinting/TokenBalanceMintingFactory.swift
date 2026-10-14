import Foundation
import Operation_iOS
import SubstrateSdk

public protocol TokenBalanceMintingFactoryProtocol {
    func createTokenMintingWrapper(
        for accountId: AccountId,
        amount: Balance,
        chainAsset: ChainAssetProtocol
    ) -> CompoundOperationWrapper<RuntimeCallCollecting>
}
