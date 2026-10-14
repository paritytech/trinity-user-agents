import Foundation
import SubstrateSdk

public final class SiDataTypeMapper: SiTypeMapping {
    public init() {}

    public func map(type: RuntimeType, identifier _: String) -> Node? {
        if type.path == ["pallet_identity", "types", "Data"] {
            DataNode()
        } else {
            nil
        }
    }
}
