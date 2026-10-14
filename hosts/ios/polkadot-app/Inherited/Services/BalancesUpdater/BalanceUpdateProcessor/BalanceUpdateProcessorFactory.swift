import Foundation
import SubstrateSdk

protocol BalanceUpdateProcessorFactoryProtocol {
    func createProcessor(for accountId: AccountId, chainAssetId: ChainAssetId) -> BalanceUpdateProcessing
}
