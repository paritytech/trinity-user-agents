import UIKit
import SubstrateSdk

struct DepositSummary {
    let depositAddress: AccountAddress
    let minimumAmount: Balance
    let rate: Decimal
    let feeInUsd: Decimal
    let qrCode: UIImage?
}
