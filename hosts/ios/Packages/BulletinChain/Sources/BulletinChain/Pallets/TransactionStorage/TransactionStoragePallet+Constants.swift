import Foundation
import SubstrateSdk

public extension TransactionStoragePallet {
    static var maxTransactionSizePath: ConstantCodingPath {
        ConstantCodingPath(moduleName: name, constantName: "MaxTransactionSize")
    }

    static var authorizationPeriodPath: ConstantCodingPath {
        ConstantCodingPath(moduleName: name, constantName: "AuthorizationPeriod")
    }
}
