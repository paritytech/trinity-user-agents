import Foundation
import SubstrateSdk
import ExtrinsicService
import KeyDerivation

public extension PGASPallet {
    final class AsPgasTxExtension {
        public let extrinsicVersion: Extrinsic.Version
        public let info: AsPgasInfo?

        public init(
            extrinsicVersion: Extrinsic.Version,
            info: AsPgasInfo?
        ) {
            self.extrinsicVersion = extrinsicVersion
            self.info = info
        }
    }

    struct AsPgasInfo {
        public let vrfManager: BandersnatchKeyManaging
        public let ringIndex: MembersPallet.RingIndex
        public let revision: UInt32
        public let proofParams: MembersProofParams
        public let collection: ResourcesPallet.MembershipCollection
        public let proofContext: Data
        public let day: UInt32

        public init(
            vrfManager: BandersnatchKeyManaging,
            ringIndex: MembersPallet.RingIndex,
            revision: UInt32,
            proofParams: MembersProofParams,
            collection: ResourcesPallet.MembershipCollection,
            proofContext: Data,
            day: UInt32
        ) {
            self.vrfManager = vrfManager
            self.ringIndex = ringIndex
            self.revision = revision
            self.proofParams = proofParams
            self.collection = collection
            self.proofContext = proofContext
            self.day = day
        }
    }
}

// MARK: - Codable Models

public extension PGASPallet.AsPgasTxExtension {
    /// AsPgasInfo::Claim { proof, ring_index, revision, collection, day }
    struct ClaimMode: Codable {
        @BytesCodable var proof: Data
        @StringCodable var ringIndex: MembersPallet.RingIndex
        @StringCodable var revision: UInt32
        let collection: PgasCollectionMode
        @StringCodable var day: UInt32
    }

    enum PgasCollectionMode: Codable {
        case people
        case litePeople

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)
            switch type {
            case "People": self = .people
            case "LitePeople": self = .litePeople
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unknown collection \(type)"
                )
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

    enum Mode: Codable {
        case claim(ClaimMode)

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)
            switch type {
            case "Claim":
                self = try .claim(container.decode(ClaimMode.self))
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unknown type \(type)"
                )
            }
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()
            switch self {
            case let .claim(model):
                try container.encode("Claim")
                try container.encode(model)
            }
        }
    }
}

// MARK: - TransactionExtending

extension PGASPallet.AsPgasTxExtension: TransactionExtending {
    public var txExtensionId: String { "AsPgas" }

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

        let proof = try info.vrfManager.createProof(
            message,
            members: info.proofParams.ringMembers,
            context: info.proofContext,
            domainSize: info.proofParams.ringSize
        )

        let collectionMode: PgasCollectionMode =
            switch info.collection {
            case .people: .people
            case .litePeople: .litePeople
            }

        let mode = Mode.claim(
            ClaimMode(
                proof: proof,
                ringIndex: info.ringIndex,
                revision: info.revision,
                collection: collectionMode,
                day: info.day
            )
        )

        let json = try mode.toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }
}
