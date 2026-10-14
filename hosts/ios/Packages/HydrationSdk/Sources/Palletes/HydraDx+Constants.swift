import Foundation
import SubstrateSdk

extension HydraDx {
    static var assetFeeParametersPath: ConstantCodingPath {
        ConstantCodingPath(moduleName: dynamicFeesModule, constantName: "AssetFeeParameters")
    }

    static var protocolFeeParametersPath: ConstantCodingPath {
        ConstantCodingPath(moduleName: dynamicFeesModule, constantName: "ProtocolFeeParameters")
    }
}
