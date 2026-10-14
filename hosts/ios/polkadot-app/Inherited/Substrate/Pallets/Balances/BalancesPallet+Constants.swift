import Foundation
import SubstrateSdk

extension BalancesPallet {
    static var existentialDepositPath: ConstantCodingPath {
        .init(moduleName: BalancesPallet.name, constantName: "ExistentialDeposit")
    }
}
