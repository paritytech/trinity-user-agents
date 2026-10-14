import Foundation
import SubstrateSdk

public struct RuntimeSnapshot {
    public let localCommonHash: String?
    public let localChainHash: String?
    public let typeRegistryCatalog: TypeRegistryCatalogProtocol
    public let specVersion: UInt32
    public let txVersion: UInt32
    public let metadata: RuntimeMetadataProtocol
}
