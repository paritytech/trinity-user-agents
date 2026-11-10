import Foundation
import SubstrateSdk
import ExtrinsicService
import KeyDerivation

public extension ResourcesPallet {
    final class AsResourcesTxExtension {
        public let extrinsicVersion: Extrinsic.Version
        public let info: AsResourcesInfo?

        public init(
            extrinsicVersion: Extrinsic.Version,
            info: AsResourcesInfo?
        ) {
            self.extrinsicVersion = extrinsicVersion
            self.info = info
        }
    }

    enum AsResourcesInfo {
        case registerStatementStoreAllowance(AsRegisterStatementStoreAllowanceParams)
        /// Same proof inputs as the statement store allowance, under the notification slot context.
        case registerNotificationForCollection(AsRegisterStatementStoreAllowanceParams)
        case claimLongTermStorage(AsClaimLongTermStorageParams)
    }

    enum MembershipCollection {
        case people
        case litePeople
    }

    struct AsRegisterStatementStoreAllowanceParams {
        public let vrfManager: BandersnatchKeyManaging
        public let ringIndex: MembersPallet.RingIndex
        public let proofParams: MembersProofParams
        public let collection: MembershipCollection
        public let proofContext: Data

        public init(
            vrfManager: BandersnatchKeyManaging,
            ringIndex: MembersPallet.RingIndex,
            proofParams: MembersProofParams,
            collection: MembershipCollection,
            proofContext: Data
        ) {
            self.vrfManager = vrfManager
            self.ringIndex = ringIndex
            self.proofParams = proofParams
            self.collection = collection
            self.proofContext = proofContext
        }
    }

    struct AsClaimLongTermStorageParams {
        public let vrfManager: BandersnatchKeyManaging
        public let ringIndex: MembersPallet.RingIndex
        public let revision: UInt32
        public let proofParams: MembersProofParams
        public let collection: MembershipCollection
        public let proofContext: Data

        public init(
            vrfManager: BandersnatchKeyManaging,
            ringIndex: MembersPallet.RingIndex,
            revision: UInt32,
            proofParams: MembersProofParams,
            collection: MembershipCollection,
            proofContext: Data
        ) {
            self.vrfManager = vrfManager
            self.ringIndex = ringIndex
            self.revision = revision
            self.proofParams = proofParams
            self.collection = collection
            self.proofContext = proofContext
        }
    }
}

// MARK: - Codable Models

public extension ResourcesPallet.AsResourcesTxExtension {
    enum Mode: Codable {
        case registerStatementStoreAllowance(RegisterStatementStoreAllowanceMode)
        case registerNotificationForCollection(RegisterStatementStoreAllowanceMode)
        case claimLongTermStorage(ClaimLongTermStorageMode)

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)
            switch type {
            case "RegisterStatementStoreAllowance":
                self = try .registerStatementStoreAllowance(container.decode(RegisterStatementStoreAllowanceMode.self))
            case "RegisterNotificationForCollection":
                self = try .registerNotificationForCollection(
                    container.decode(RegisterStatementStoreAllowanceMode.self)
                )
            case "ClaimLongTermStorage":
                self = try .claimLongTermStorage(container.decode(ClaimLongTermStorageMode.self))
            default:
                throw DecodingError.dataCorruptedError(in: container, debugDescription: "Unknown type \(type)")
            }
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()
            switch self {
            case let .registerStatementStoreAllowance(model):
                try container.encode("RegisterStatementStoreAllowance")
                try container.encode(model)
            case let .registerNotificationForCollection(model):
                try container.encode("RegisterNotificationForCollection")
                try container.encode(model)
            case let .claimLongTermStorage(model):
                try container.encode("ClaimLongTermStorage")
                try container.encode(model)
            }
        }
    }

    // RegisterStatementStoreAllowance(ProofOf<T>, RingIndex, RevisionIndex, MembershipCollection)
    struct RegisterStatementStoreAllowanceMode: Codable {
        enum CodingKeys: String, CodingKey {
            case proof = "0"
            case ringIndex = "1"
            case revision = "2"
            case collection = "3"
        }

        @BytesCodable var proof: Data
        @StringCodable var ringIndex: MembersPallet.RingIndex
        @StringCodable var revision: UInt32
        let collection: MembershipCollectionMode
    }

    // ClaimLongTermStorage(ProofOf<T>, RingIndex, RevisionIndex, MembershipCollection)
    struct ClaimLongTermStorageMode: Codable {
        enum CodingKeys: String, CodingKey {
            case proof = "0"
            case ringIndex = "1"
            case revision = "2"
            case collection = "3"
        }

        @BytesCodable var proof: Data
        @StringCodable var ringIndex: MembersPallet.RingIndex
        @StringCodable var revision: UInt32
        let collection: MembershipCollectionMode
    }

    enum MembershipCollectionMode: Codable {
        case people
        case litePeople

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)
            switch type {
            case "People": self = .people
            case "LitePeople": self = .litePeople
            default:
                throw DecodingError.dataCorruptedError(in: container, debugDescription: "Unknown collection \(type)")
            }
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()
            switch self {
            case .people:
                try container.encode("People")
                try container.encode(JSON.null)
            case .litePeople:
                try container.encode("LitePeople")
                try container.encode(JSON.null)
            }
        }
    }
}

// MARK: - TransactionExtending

extension ResourcesPallet.AsResourcesTxExtension: TransactionExtending {
    public var txExtensionId: String { "AsResources" }

    public func implicit(
        using _: DynamicScaleEncodingFactoryProtocol,
        metadata _: RuntimeMetadataProtocol,
        extensionVersion _: UInt8,
        context _: RuntimeJsonContext?
    ) throws -> Data? {
        nil
    }

    public func explicit(
        for implication: TransactionExtension.Implication,
        encodingFactory: DynamicScaleEncodingFactoryProtocol,
        metadata: RuntimeMetadataProtocol,
        context: RuntimeJsonContext?
    ) throws -> TransactionExtension.Explicit? {
        guard let info else {
            return TransactionExtension.Explicit(
                from: .null,
                txExtensionId: txExtensionId,
                metadata: metadata
            )
        }

        let payloadFactory = ImplicationSignaturePayloadFactory(formatVersion: extrinsicVersion.formatVersion)
        let payload = try payloadFactory.createPayload(from: implication, using: encodingFactory)
        let message = try payload.blake2b32()

        let mode: Mode =
            switch info {
            case let .registerStatementStoreAllowance(params):
                try .registerStatementStoreAllowance(makeAllowanceMode(params: params, message: message))
            case let .registerNotificationForCollection(params):
                try .registerNotificationForCollection(makeAllowanceMode(params: params, message: message))
            case let .claimLongTermStorage(params):
                try makeClaimLongTermStorage(params: params, message: message)
            }

        let json = try mode.toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }
}

// MARK: - Private

private extension ResourcesPallet.AsResourcesTxExtension {
    func makeAllowanceMode(
        params: ResourcesPallet.AsRegisterStatementStoreAllowanceParams,
        message: Data
    ) throws -> RegisterStatementStoreAllowanceMode {
        let proof = try params.vrfManager.createProof(
            message,
            members: params.proofParams.ringMembers,
            context: params.proofContext,
            domainSize: params.proofParams.ringSize
        )
        return RegisterStatementStoreAllowanceMode(
            proof: proof,
            ringIndex: params.ringIndex,
            revision: params.proofParams.revision,
            collection: params.collection.asMode
        )
    }

    func makeClaimLongTermStorage(
        params: ResourcesPallet.AsClaimLongTermStorageParams,
        message: Data
    ) throws -> Mode {
        let proof = try params.vrfManager.createProof(
            message,
            members: params.proofParams.ringMembers,
            context: params.proofContext,
            domainSize: params.proofParams.ringSize
        )
        return .claimLongTermStorage(
            ClaimLongTermStorageMode(
                proof: proof,
                ringIndex: params.ringIndex,
                revision: params.revision,
                collection: params.collection.asMode
            )
        )
    }
}

private extension ResourcesPallet.MembershipCollection {
    var asMode: ResourcesPallet.AsResourcesTxExtension.MembershipCollectionMode {
        switch self {
        case .people: .people
        case .litePeople: .litePeople
        }
    }
}
