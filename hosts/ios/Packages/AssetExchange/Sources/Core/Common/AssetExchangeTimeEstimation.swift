import Foundation
import Operation_iOS
import SubstrateSdk

public protocol AssetExchangeTimeEstimating {
    func totalTimeWrapper(for chainIds: [ChainId]) -> CompoundOperationWrapper<TimeInterval>
}
