import Foundation
import SubstrateSdk

extension BabePallet {
    static var babeBlockTime: ConstantCodingPath {
        ConstantCodingPath(moduleName: name, constantName: "ExpectedBlockTime")
    }
}
