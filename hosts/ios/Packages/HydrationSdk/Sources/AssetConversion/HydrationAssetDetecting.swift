import Foundation
import SubstrateSdk

public protocol HydrationAssetDetecting {
    func canPayFee(using asset: ChainAssetProtocol) -> Bool
}
