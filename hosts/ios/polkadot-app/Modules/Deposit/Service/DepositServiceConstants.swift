import Foundation
import SubstrateSdk

enum DepositServiceConstants {
    static let slippage = BigRational(numerator: 5, denominator: 1_000)
    static let midDepositInUsd: Decimal = 100
    // start deposit if the amount is higher than 80% of min deposit
    static let depositInitThreshold = BigRational.percent(of: 80)
    static let feeBufferPercentage = BigRational.percent(of: 10)
}
