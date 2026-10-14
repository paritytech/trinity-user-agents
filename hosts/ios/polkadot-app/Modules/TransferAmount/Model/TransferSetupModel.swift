import Foundation
import BigInt
import SubstrateSdk

enum TransferSetupModel {
    case concrete(amount: BigUInt)
    case all
}
