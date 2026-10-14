import Foundation
import SubstrateSdk

struct DataChannelMessage: Equatable {
    let id: String
    let data: Data
}

extension DataChannelMessage: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        id = try String(scaleDecoder: scaleDecoder)
        data = try Data(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try id.encode(scaleEncoder: scaleEncoder)
        try data.encode(scaleEncoder: scaleEncoder)
    }
}
