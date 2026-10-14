import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateSdkExt

public enum MembersSubscriberPallet {
    public static let name = "MembersSubscriber"
}

// MARK: - Storage

public extension MembersSubscriberPallet {
    enum Storage {
        case currentGeneration
        case ringRoots
    }
}

extension MembersSubscriberPallet.Storage: StoragePathConvertible {
    public var name: String {
        switch self {
        case .currentGeneration:
            "CurrentGeneration"
        case .ringRoots:
            "RingRoots"
        }
    }

    public var moduleName: String { MembersSubscriberPallet.name }
}

// MARK: - Types

public extension MembersSubscriberPallet {
    typealias Generation = UInt32

    struct RingCommitmentRecord: Decodable {
        @BytesCodable public var root: Data
        @StringCodable public var revision: UInt32
    }

    struct RingRootsKey: NMapKeyStorageKeyProtocol {
        public let generation: Generation
        public let collectionId: Data
        public let ringIndex: MembersPallet.RingIndex

        public init(generation: Generation, collectionId: Data, ringIndex: MembersPallet.RingIndex) {
            self.generation = generation
            self.collectionId = collectionId
            self.ringIndex = ringIndex
        }

        public func appendSubkey(to encoder: DynamicScaleEncoding, type: String, index: Int) throws {
            switch index {
            case 0:
                try encoder.append(StringCodable(wrappedValue: generation), ofType: type)
            case 1:
                try encoder.append(BytesCodable(wrappedValue: collectionId), ofType: type)
            case 2:
                try encoder.append(StringCodable(wrappedValue: ringIndex), ofType: type)
            default:
                break
            }
        }
    }
}
