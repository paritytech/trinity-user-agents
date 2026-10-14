import Foundation
import SubstrateSdk

struct DepositServiceInfo {
    let amountIn: Balance
    let amountOut: Balance
    let feeInUsd: Decimal
    let minDeposit: Balance
    let walletToDeposit: MetaAccountModelProtocol
}
