import Foundation
import SubstrateSdk

public extension ScaleDecodable {
    static func fromScaleEncoded(_ data: Data) throws -> Self {
        let scaleDecoder = try ScaleDecoder(data: data)
        return try Self(scaleDecoder: scaleDecoder)
    }
}
