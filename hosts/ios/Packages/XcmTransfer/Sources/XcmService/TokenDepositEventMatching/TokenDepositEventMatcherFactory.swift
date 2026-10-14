import Foundation
import SubstrateSdk
import SDKLogger

public protocol TokenDepositEventMatcherFactoryProtocol {
    func createMatcher(for chainAsset: ChainAssetProtocol) -> [TokenDepositEventMatching]?
}
