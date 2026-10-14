import Foundation
import SubstrateSdk

/// RFC-0004 stable ring address: a required chain and a junction path identifying the ring within it.
public struct RingLocation: Hashable, Codable {
    @HexCodable public var chainId: Data
    public let junctions: [RingLocationJunction]

    public init(chainId: Data, junctions: [RingLocationJunction]) {
        _chainId = HexCodable(wrappedValue: chainId)
        self.junctions = junctions
    }

    public var palletInstance: UInt8? {
        for case let .palletInstance(index) in junctions {
            return index
        }

        return nil
    }

    public var collectionId: Data? {
        for case let .collectionId(bytes) in junctions {
            return bytes
        }

        return nil
    }
}

public enum RingLocationJunction: Hashable {
    case palletInstance(UInt8)
    case collectionId(Data)
}

extension RingLocationJunction: Codable {
    private enum CodingKeys: String, CodingKey {
        case tag
        case value
    }

    private enum Tag: String, Codable {
        case palletInstance = "PalletInstance"
        case collectionId = "CollectionId"
    }

    public init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)

        switch try container.decode(Tag.self, forKey: .tag) {
        case .palletInstance:
            self = try .palletInstance(container.decode(UInt8.self, forKey: .value))
        case .collectionId:
            let bytes = try container.decode(HexCodable<Data>.self, forKey: .value).wrappedValue
            self = .collectionId(bytes)
        }
    }

    public func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)

        switch self {
        case let .palletInstance(index):
            try container.encode(Tag.palletInstance, forKey: .tag)
            try container.encode(index, forKey: .value)
        case let .collectionId(bytes):
            try container.encode(Tag.collectionId, forKey: .tag)
            try container.encode(HexCodable(wrappedValue: bytes), forKey: .value)
        }
    }
}
