import Foundation
import SubstrateSdk
import KeyDerivation

public extension PeoplePallet {
    final class AsPersonTxExtension {
        public let usability: Usability
        public let extrinsicVersion: Extrinsic.Version

        public init(
            extrinsicVersion: Extrinsic.Version,
            usability: Usability
        ) {
            self.extrinsicVersion = extrinsicVersion
            self.usability = usability
        }
    }
}

public extension PeoplePallet.AsPersonTxExtension {
    struct AsPersonalAliasWithProofMode: Codable {
        enum CodingKeys: String, CodingKey {
            case proof = "0"
            case ringIndex = "1"
            case revision = "2"
            case context = "3"
        }

        @BytesCodable var proof: Data
        @StringCodable var ringIndex: MembersPallet.RingIndex
        @StringCodable var revision: MembersPallet.RevisionIndex
        @BytesCodable var context: Data
    }

    struct AsPersonalAliasWithProofUsability {
        let vrfManager: BandersnatchKeyManaging
        let ringIndex: MembersPallet.RingIndex
        let proofParams: MembersProofParams
        let context: Data

        public init(
            vrfManager: BandersnatchKeyManaging,
            ringIndex: MembersPallet.RingIndex,
            proofParams: MembersProofParams,
            context: Data
        ) {
            self.vrfManager = vrfManager
            self.ringIndex = ringIndex
            self.proofParams = proofParams
            self.context = context
        }
    }

    struct AsPersonalIdentityWithProofMode: Codable {
        enum CodingKeys: String, CodingKey {
            case signature = "0"
            case personalId = "1"
        }

        let signature: Data
        @StringCodable var personalId: PeoplePallet.PersonalId
    }

    struct AsPersonalIdentityWithProofUsability {
        public let vrfManager: BandersnatchKeyManaging
        public let personalId: PeoplePallet.PersonalId

        public init(vrfManager: BandersnatchKeyManaging, personalId: PeoplePallet.PersonalId) {
            self.vrfManager = vrfManager
            self.personalId = personalId
        }
    }

    struct AsPersonalAliasWithAccountRevisedMode: Codable {
        enum CodingKeys: String, CodingKey {
            case nonce = "0"
            case proof = "1"
            case ringIndex = "2"
            case revision = "3"
            case context = "4"
        }

        @StringCodable var nonce: AccountNonce
        @BytesCodable var proof: Data
        @StringCodable var ringIndex: MembersPallet.RingIndex
        @StringCodable var revision: MembersPallet.RevisionIndex
        @BytesCodable var context: Data
    }

    struct AsPersonAliasWithAccountRevisedUsability {
        let nonce: AccountNonce
        let accountId: AccountId
        let vrfManager: BandersnatchKeyManaging
        let ringIndex: MembersPallet.RingIndex
        let proofParams: MembersProofParams
        let context: Data

        public init(
            nonce: AccountNonce,
            accountId: AccountId,
            vrfManager: BandersnatchKeyManaging,
            ringIndex: MembersPallet.RingIndex,
            proofParams: MembersProofParams,
            context: Data
        ) {
            self.nonce = nonce
            self.accountId = accountId
            self.vrfManager = vrfManager
            self.ringIndex = ringIndex
            self.proofParams = proofParams
            self.context = context
        }
    }

    enum Mode: Codable {
        case asPersonalAliasWithAccount(AccountNonce)
        case asPersonalAliasWithProof(AsPersonalAliasWithProofMode)
        case asPersonalIdentityWithProof(AsPersonalIdentityWithProofMode)
        case asPersonalIdentityWithAccount(AccountNonce)
        case asPersonalAliasWithAccountRevised(AsPersonalAliasWithAccountRevisedMode)

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()

            let type = try container.decode(String.self)

            switch type {
            case "AsPersonalAliasWithAccount":
                let nonce = try container.decode(StringScaleMapper<AccountNonce>.self).value
                self = .asPersonalAliasWithAccount(nonce)
            case "AsPersonalAliasWithProof":
                let model = try container.decode(AsPersonalAliasWithProofMode.self)
                self = .asPersonalAliasWithProof(model)
            case "AsPersonalIdentityWithProof":
                let model = try container.decode(AsPersonalIdentityWithProofMode.self)
                self = .asPersonalIdentityWithProof(model)
            case "AsPersonalIdentityWithAccount":
                let nonce = try container.decode(StringScaleMapper<AccountNonce>.self).value
                self = .asPersonalIdentityWithAccount(nonce)
            case "AsPersonalAliasWithAccountRevised":
                let model = try container.decode(AsPersonalAliasWithAccountRevisedMode.self)
                self = .asPersonalAliasWithAccountRevised(model)
            default:
                throw DecodingError.dataCorruptedError(
                    in: container,
                    debugDescription: "Unknown mode \(type)"
                )
            }
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()

            switch self {
            case let .asPersonalAliasWithProof(model):
                try container.encode("AsPersonalAliasWithProof")
                try container.encode(model)
            case let .asPersonalAliasWithAccount(nonce):
                try container.encode("AsPersonalAliasWithAccount")
                try container.encode(StringScaleMapper(value: nonce))
            case let .asPersonalIdentityWithProof(model):
                try container.encode("AsPersonalIdentityWithProof")
                try container.encode(model)
            case let .asPersonalIdentityWithAccount(nonce):
                try container.encode("AsPersonalIdentityWithAccount")
                try container.encode(StringScaleMapper(value: nonce))
            case let .asPersonalAliasWithAccountRevised(model):
                try container.encode("AsPersonalAliasWithAccountRevised")
                try container.encode(model)
            }
        }
    }

    enum Usability {
        case asPersonalAliasWithProof(AsPersonalAliasWithProofUsability)
        case asPersonalAliasWithAccount(AccountNonce)
        case asPersonalIdentityWithProof(AsPersonalIdentityWithProofUsability)
        case asPersonalIdentityWithAccount(AccountNonce)
        case asPersonalAliasWithAccountRevised(AsPersonAliasWithAccountRevisedUsability)
    }
}

extension PeoplePallet.AsPersonTxExtension: TransactionExtending {
    public var txExtensionId: String { "AsPerson" }

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
        switch usability {
        case let .asPersonalAliasWithAccount(nonce):
            try TransactionExtension.Explicit(
                from: Mode.asPersonalAliasWithAccount(nonce)
                    .toScaleCompatibleJSON(with: context?.toRawContext()),
                txExtensionId: txExtensionId,
                metadata: metadata
            )
        case let .asPersonalIdentityWithAccount(nonce):
            try TransactionExtension.Explicit(
                from: Mode.asPersonalIdentityWithAccount(nonce)
                    .toScaleCompatibleJSON(with: context?.toRawContext()),
                txExtensionId: txExtensionId,
                metadata: metadata
            )
        case let .asPersonalAliasWithProof(params):
            try makeAsPersonalAliasWithProof(
                params: params,
                implication: implication,
                encodingFactory: encodingFactory,
                metadata: metadata,
                context: context
            )
        case let .asPersonalIdentityWithProof(params):
            try makeAsPersonalIdentityWithProof(
                params: params,
                implication: implication,
                encodingFactory: encodingFactory,
                metadata: metadata,
                context: context
            )
        case let .asPersonalAliasWithAccountRevised(params):
            try makeAsPersonalAliasWithAccountRevisedExplicit(
                params: params,
                implication: implication,
                encodingFactory: encodingFactory,
                metadata: metadata,
                context: context
            )
        }
    }

    private func makeAsPersonalAliasWithProof(
        params: AsPersonalAliasWithProofUsability,
        implication: TransactionExtension.Implication,
        encodingFactory: DynamicScaleEncodingFactoryProtocol,
        metadata: RuntimeMetadataProtocol,
        context: RuntimeJsonContext?
    ) throws -> TransactionExtension.Explicit? {
        let payloadFactory = ImplicationSignaturePayloadFactory(formatVersion: extrinsicVersion.formatVersion)
        let payload = try payloadFactory.createPayload(from: implication, using: encodingFactory)
        let message = try payload.blake2b32()

        let proof = try params.vrfManager.createProof(
            message,
            members: params.proofParams.ringMembers,
            context: params.context,
            domainSize: params.proofParams.ringSize
        )

        let model = AsPersonalAliasWithProofMode(
            proof: proof,
            ringIndex: params.ringIndex,
            revision: params.proofParams.revision,
            context: params.context
        )

        let json = try Mode.asPersonalAliasWithProof(model)
            .toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }

    private func makeAsPersonalIdentityWithProof(
        params: AsPersonalIdentityWithProofUsability,
        implication: TransactionExtension.Implication,
        encodingFactory: DynamicScaleEncodingFactoryProtocol,
        metadata: RuntimeMetadataProtocol,
        context: RuntimeJsonContext?
    ) throws -> TransactionExtension.Explicit? {
        let payloadFactory = ImplicationSignaturePayloadFactory(formatVersion: extrinsicVersion.formatVersion)
        let payload = try payloadFactory.createPayload(from: implication, using: encodingFactory)
        let message = try payload.blake2b32()

        let signature = try params.vrfManager.sign(message)

        let model = AsPersonalIdentityWithProofMode(
            signature: signature,
            personalId: params.personalId
        )

        let json = try Mode.asPersonalIdentityWithProof(model)
            .toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }

    private func makeAsPersonalAliasWithAccountRevisedExplicit(
        params: AsPersonAliasWithAccountRevisedUsability,
        implication: TransactionExtension.Implication,
        encodingFactory: DynamicScaleEncodingFactoryProtocol,
        metadata: RuntimeMetadataProtocol,
        context: RuntimeJsonContext?
    ) throws -> TransactionExtension.Explicit? {
        let payloadFactory = ImplicationSignaturePayloadFactory(formatVersion: extrinsicVersion.formatVersion)
        let implicationData = try payloadFactory.createPayload(from: implication, using: encodingFactory)
        let reviseData = try "revise".scaleEncoded()
        let nonceData = try params.nonce.scaleEncoded()
        let payload = implicationData + reviseData + params.accountId + nonceData
        let message = try payload.blake2b32()

        let proof = try params.vrfManager.createProof(
            message,
            members: params.proofParams.ringMembers,
            context: params.context,
            domainSize: params.proofParams.ringSize
        )

        let model = AsPersonalAliasWithAccountRevisedMode(
            nonce: params.nonce,
            proof: proof,
            ringIndex: params.ringIndex,
            revision: params.proofParams.revision,
            context: params.context
        )

        let json = try Mode.asPersonalAliasWithAccountRevised(model)
            .toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }
}
