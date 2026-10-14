import Foundation
import SubstrateSdk
import SubstrateStorageQuery
import SubstrateSdkExt

public enum MembersPallet {
    public static let name = "Members"

    public typealias RingIndex = UInt32
    public typealias PageIndex = UInt32
    public typealias RingMember = Data
    public typealias RevisionIndex = UInt32
    public typealias CollectionIdentifier = Data
}

// MARK: - Storage

public extension MembersPallet {
    enum Storage {
        case members
        case ringKeys
        case ringKeysStatus
        case root
        case collections
        case ringsState
    }
}

extension MembersPallet.Storage: StoragePathConvertible {
    public var name: String {
        switch self {
        case .members:
            "Members"
        case .ringKeys:
            "RingKeys"
        case .ringKeysStatus:
            "RingKeysStatus"
        case .root:
            "Root"
        case .collections:
            "Collections"
        case .ringsState:
            "RingsState"
        }
    }

    public var moduleName: String { MembersPallet.name }
}

// MARK: - RingStatus (replaces RecyclerState)

public extension MembersPallet {
    struct RingStatus: Decodable {
        @StringCodable public var total: UInt32
        @StringCodable public var included: UInt32

        public init(total: UInt32, included: UInt32) {
            _total = StringCodable(wrappedValue: total)
            _included = StringCodable(wrappedValue: included)
        }
    }
}

// MARK: - RingPosition (for pending detection)

public extension MembersPallet {
    enum RingPosition: Decodable, Equatable {
        public struct Included: Decodable, Equatable {
            @StringCodable public var ringIndex: RingIndex
            @StringCodable public var ringPage: PageIndex
            @StringCodable public var ringPosition: UInt32

            public init(ringIndex: RingIndex, ringPage: PageIndex, ringPosition: UInt32) {
                _ringIndex = StringCodable(wrappedValue: ringIndex)
                _ringPage = StringCodable(wrappedValue: ringPage)
                _ringPosition = StringCodable(wrappedValue: ringPosition)
            }
        }

        public struct Onboarding: Decodable, Equatable {
            @StringCodable public var queuePage: PageIndex
            @StringCodable public var queuedAt: UInt64

            public init(queuePage: PageIndex, queuedAt: UInt64) {
                _queuePage = StringCodable(wrappedValue: queuePage)
                _queuedAt = StringCodable(wrappedValue: queuedAt)
            }
        }

        case onboarding(Onboarding)
        case included(Included)
        case suspended

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)

            switch type {
            case "Onboarding":
                let model = try container.decode(Onboarding.self)
                self = .onboarding(model)
            case "Included":
                let model = try container.decode(Included.self)
                self = .included(model)
            case "Suspended":
                self = .suspended
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unexpected RingPosition type \(type)"
                )
            }
        }

        public var isIncluded: Bool {
            switch self {
            case .included: true
            case .onboarding,
                 .suspended: false
            }
        }

        public var isSuspended: Bool {
            switch self {
            case .suspended: true
            case .onboarding,
                 .included: false
            }
        }

        public var ringIndex: RingIndex? {
            switch self {
            case .onboarding:
                nil
            case let .included(included):
                included.ringIndex
            case .suspended:
                nil
            }
        }

        public var inclusion: Included? {
            switch self {
            case .onboarding,
                 .suspended:
                nil
            case let .included(included):
                included
            }
        }

        public var isOnboarding: Bool {
            switch self {
            case .onboarding: true
            case .included,
                 .suspended: false
            }
        }

        public var onboardingQueuedAt: UInt64? {
            switch self {
            case let .onboarding(onboarding): onboarding.queuedAt
            case .included,
                 .suspended: nil
            }
        }
    }
}

// MARK: - NMap Keys for RingKeys queries

public extension MembersPallet {
    struct RingKeys {
        public let allMembers: [RingMember]
        public let includedCount: UInt32

        public init(allMembers: [RingMember], includedCount: UInt32) {
            self.allMembers = allMembers
            self.includedCount = includedCount
        }

        public var includedMembers: [RingMember] { Array(allMembers.prefix(Int(includedCount))) }
    }
}

public extension MembersPallet {
    struct RingRoot: Decodable, Equatable {
        @StringCodable public var revision: UInt32
    }
}

public extension MembersPallet {
    enum RingMutationMode: Decodable, Equatable {
        case appendOnly
        case other(type: String)

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)
            switch type {
            case "AppendOnly":
                self = .appendOnly
            default:
                self = .other(type: type)
            }
        }

        var isAppendOnly: Bool {
            self == .appendOnly
        }
    }

    struct RingMembersState: Decodable, Equatable {
        public let mode: RingMutationMode

        public var appendOnly: Bool { mode.isAppendOnly }
    }
}

public extension MembersPallet {
    struct RingKeysStatus: Decodable, Equatable {
        @StringCodable public var total: UInt32
        @StringCodable public var included: UInt32

        public init(total: UInt32, included: UInt32) {
            _total = StringCodable(wrappedValue: total)
            _included = StringCodable(wrappedValue: included)
        }

        /// Whether the key sitting at `ringPosition` is already baked into the ring root.
        ///
        /// `keysPerPage` comes from `fetchRingKeysPageSize()`: a member's position counts within its own
        /// ring page, while `included` counts across the whole ring, so the pages before the member's own
        /// have to be added back before the two can be compared.
        public func includesKey(from ringPosition: RingPosition, keysPerPage: Int) -> Bool {
            guard let inclusion = ringPosition.inclusion else {
                return false
            }

            return includesKey(at: inclusion, keysPerPage: keysPerPage)
        }

        public func includesKey(at ringPosition: RingPosition.Included, keysPerPage: Int) -> Bool {
            Int(included) > Int(ringPosition.ringPage) * keysPerPage + Int(ringPosition.ringPosition)
        }
    }
}
