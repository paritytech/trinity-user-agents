import Foundation
import SubstrateSdk

extension ScaleDecodable {
    static func scaleDecode(from data: Data) throws -> Self {
        let decoder = try ScaleDecoder(data: data)
        return try Self(scaleDecoder: decoder)
    }
}
