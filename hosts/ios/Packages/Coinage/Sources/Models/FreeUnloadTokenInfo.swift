import Foundation
import SubstrateSdk

public struct FreeUnloadTokenInfo: Decodable {
    public let people: UInt32
    public let litePeople: UInt32

    public init(from decoder: Decoder) throws {
        var container = try decoder.unkeyedContainer()
        people = try container.decode(StringScaleMapper<UInt32>.self).value
        litePeople = try container.decode(StringScaleMapper<UInt32>.self).value
    }
}
