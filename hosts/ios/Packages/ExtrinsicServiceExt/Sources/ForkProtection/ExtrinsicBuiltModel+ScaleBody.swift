import Foundation
import SubstrateSdk
import ExtrinsicService

extension ExtrinsicBuiltModel {
    var scaleBody: Data {
        get throws {
            let full = try Data(hexString: extrinsic)
            return try Data(scaleDecoder: ScaleDecoder(data: full))
        }
    }
}
