import Foundation
import SubstrateSdk
import AssetExchange

final class AssetExchangeSufficiencyProvider: AssetExchangeSufficiencyProviding {
    func isSufficient(asset _: AssetProtocol) -> Bool {
        true
    }
}
