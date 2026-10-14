import Foundation
import SubstrateSdk

public protocol AssetExchangeSufficiencyProviding {
    func isSufficient(asset: AssetProtocol) -> Bool
}
