import Foundation
import Operation_iOS

public struct RuntimeMetadataItem: Codable & Equatable {
    public enum CodingKeys: String, CodingKey {
        case chain
        case version
        case txVersion
        case localMigratorVersion
        case opaque
        case metadata
    }

    public let chain: String
    public let version: UInt32
    public let txVersion: UInt32
    public let localMigratorVersion: UInt32
    public let opaque: Bool
    public let metadata: Data

    public init(
        chain: String,
        version: UInt32,
        txVersion: UInt32,
        localMigratorVersion: UInt32,
        opaque: Bool,
        metadata: Data
    ) {
        self.chain = chain
        self.version = version
        self.txVersion = txVersion
        self.localMigratorVersion = localMigratorVersion
        self.opaque = opaque
        self.metadata = metadata
    }
}

extension RuntimeMetadataItem: Identifiable {
    public var identifier: String { chain }
}
