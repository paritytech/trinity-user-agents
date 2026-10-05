import Foundation
import ExtrinsicService
import SubstrateSdk
import KeyDerivation
import NovaCrypto
import Keystore_iOS
import Foundation_iOS
import Individuality

extension CoinagePallet {
    /// Transaction extension for coinage operations.
    /// Allows authentication as Origin::Coin or Origin::UnloadToken.
    final class AsCoinageTxExtension {
        let extrinsicVersion: Extrinsic.Version
        let info: AsCoinageInfo?

        init(
            extrinsicVersion: Extrinsic.Version = .V5(extensionVersion: 0),
            info: AsCoinageInfo?
        ) {
            self.extrinsicVersion = extrinsicVersion
            self.info = info
        }
    }

    enum AsCoinageInfo {
        case asCoin

        case asUnloadTokenPeople(AsUnloadTokenPeopleParams)

        case asUnloadTokenLitePeople(AsUnloadTokenPeopleParams)

        case asUnloadTokenPaid(AsUnloadTokenPaidParams)

        case asUnloadTokenFromOutput(AsUnloadTokenFromOutputParams)

        case infallibleUnpaidSigned(UInt32)
    }

    struct AsUnloadTokenPeopleParams {
        /// Key manager for generating Ring-VRF proof.
        let keyManager: any BandersnatchKeyManaging
        /// Proof params for the personhood Ring-VRF proof.
        let peopleProofParams: MembersProofParams
        /// Member's ring index
        let peopleRingIndex: UInt32
        /// The selected (unconsumed) unload token
        let unloadToken: ResolvedUnloadToken
        /// Key managers for recycler alias proofs (one per voucher).
        let voucherKeyManagers: [any BandersnatchKeyManaging]
        /// Proof params for the recycler ring (used for alias proofs).
        let recyclerProofParams: MembersProofParams
    }

    struct AsUnloadTokenPaidParams {
        /// Ring-VRF proof from paid unload token ring.
        let proof: Data
        /// The period of the unload token.
        let period: UInt32
        let paidTokenRingIndex: UInt32
        let paidTokenRingRevision: UInt32
        /// Alias proofs for recycler aliases.
        let aliasProofs: [Data]
    }

    struct AsUnloadTokenFromOutputParams {
        let feeRecyclerValue: Int8
        let feeRecyclerIndex: UInt32
        let feeRecyclerRevision: UInt32
        /// Expected next retry counter; must equal chain state or mismatch triggers AliasTemporarilyLocked.
        /// Signed intent message is (aliasProofs[1...], retryCounter, inheritedImplication).
        ///
        /// Source from `RecyclerAliasStates[(feeRecyclerValue, feeRecyclerIndex, firstAlias)]`:
        /// 0 when the entry is absent or `.unloaded`, `retries + 1` when `.locked`.
        ///
        /// The same entry also gates spendability, and not only for the fee alias: the chain runs
        /// `ensure_alias_available` inside every alias proof validation, rejecting a `.locked` alias
        /// while `now < LockInfo.until`. `.locked` is written only when a from-output-fee unload
        /// fails after prepare, and the lock period grows as `2^retries * base`.
        ///
        /// Wiring this path therefore also requires excluding locked vouchers from the spendable
        /// set (`VoucherLocationService`, `VoucherService`, `ExternalPaymentPlanner`), which needs a
        /// `lockedUntil` on `Voucher` and a CoreData version bump. Compare against the chain's
        /// `Timestamp.Now` rather than the device clock — the chain uses `UnixTime::now()`.
        let retryCounter: UInt8
        /// All alias proofs including the first one (fee coin).
        let aliasProofs: [Data]
    }
}

// MARK: - Codable Models for Scale Encoding

extension CoinagePallet {
    enum AsCoinageInfoMode: Codable {
        case asCoin
        case asUnloadTokenPeople(AsUnloadTokenPeopleMode)
        case asUnloadTokenLitePeople(AsUnloadTokenPeopleMode)
        case asUnloadTokenPaid(AsUnloadTokenPaidMode)
        case asUnloadTokenFromOutput(AsUnloadTokenFromOutputMode)
        case infallibleUnpaidSigned(InfallibleUnpaid)

        init(from decoder: Decoder) throws {
            var container = try decoder.unkeyedContainer()
            let type = try container.decode(String.self)

            switch type {
            case "AsCoin":
                self = .asCoin
            case "AsUnloadTokenPeople":
                self = try .asUnloadTokenPeople(container.decode(AsUnloadTokenPeopleMode.self))
            case "AsUnloadTokenLitePeople":
                self = try .asUnloadTokenLitePeople(container.decode(AsUnloadTokenPeopleMode.self))
            case "AsUnloadTokenPaid":
                self = try .asUnloadTokenPaid(container.decode(AsUnloadTokenPaidMode.self))
            case "AsUnloadTokenFromOutput":
                self = try .asUnloadTokenFromOutput(container.decode(AsUnloadTokenFromOutputMode.self))
            case "InfallibleUnpaidSigned":
                self = try .infallibleUnpaidSigned(container.decode(InfallibleUnpaid.self))
            default:
                throw DecodingError.dataCorruptedError(in: container, debugDescription: "Unknown type \(type)")
            }
        }

        func encode(to encoder: Encoder) throws {
            var container = encoder.unkeyedContainer()

            switch self {
            case .asCoin:
                try container.encode("AsCoin")
                try container.encode(JSON.null)
            case let .asUnloadTokenPeople(params):
                try container.encode("AsUnloadTokenPeople")
                try container.encode(params)
            case let .asUnloadTokenLitePeople(params):
                try container.encode("AsUnloadTokenLitePeople")
                try container.encode(params)
            case let .asUnloadTokenPaid(params):
                try container.encode("AsUnloadTokenPaid")
                try container.encode(params)
            case let .asUnloadTokenFromOutput(params):
                try container.encode("AsUnloadTokenFromOutput")
                try container.encode(params)
            case let .infallibleUnpaidSigned(params):
                try container.encode("InfallibleUnpaidSigned")
                try container.encode(params)
            }
        }
    }

    struct AsUnloadTokenPeopleMode: Codable {
        let proof: PeopleProof
        @StringCodable var period: UInt32
        @StringCodable var counter: UInt32
        let aliasProofs: [BytesCodable]
    }

    struct PeopleProof: Codable {
        @BytesCodable var proof: Data
        @StringCodable var ring: UInt32
        @StringCodable var revision: UInt32
    }

    struct AsUnloadTokenPaidMode: Codable {
        @BytesCodable var proof: Data
        @StringCodable var period: UInt32
        @StringCodable var paidTokenRingIndex: UInt32
        @StringCodable var paidTokenRingRevision: UInt32
        let aliasProofs: [BytesCodable]
    }

    struct AsUnloadTokenFromOutputMode: Codable {
        @StringCodable var feeRecyclerValue: Int8
        @StringCodable var feeRecyclerIndex: UInt32
        @StringCodable var feeRecyclerRevision: UInt32
        @StringCodable var retryCounter: UInt8
        let aliasProofs: [BytesCodable]
    }

    struct InfallibleUnpaid: Codable {
        @StringCodable var nonce: UInt32
    }
}

// MARK: - TransactionExtending

extension CoinagePallet.AsCoinageTxExtension: TransactionExtending {
    private enum AsCoinageTxExtensionError: Error {
        case missingRequiredData
    }

    var txExtensionId: String { "AsCoinage" }

    func implicit(
        using _: DynamicScaleEncodingFactoryProtocol,
        metadata _: RuntimeMetadataProtocol,
        extensionVersion _: UInt8,
        context _: RuntimeJsonContext?
    ) throws -> Data? {
        nil
    }

    func explicit(
        for implication: TransactionExtension.Implication,
        encodingFactory: DynamicScaleEncodingFactoryProtocol,
        metadata: RuntimeMetadataProtocol,
        context: RuntimeJsonContext?
    ) throws -> TransactionExtension.Explicit? {
        guard let info else {
            let json = JSON.null
            return TransactionExtension.Explicit(
                from: json,
                txExtensionId: txExtensionId,
                metadata: metadata
            )
        }

        let mode: CoinagePallet.AsCoinageInfoMode =
            switch info {
            case .asCoin:
                .asCoin

            case let .asUnloadTokenPeople(params):
                try .asUnloadTokenPeople(
                    makePeopleMode(
                        params: params,
                        implication: implication,
                        encodingFactory: encodingFactory
                    )
                )

            case let .asUnloadTokenLitePeople(params):
                try .asUnloadTokenLitePeople(
                    makePeopleMode(
                        params: params,
                        implication: implication,
                        encodingFactory: encodingFactory
                    )
                )

            case let .asUnloadTokenPaid(params):
                .asUnloadTokenPaid(CoinagePallet.AsUnloadTokenPaidMode(
                    proof: params.proof,
                    period: params.period,
                    paidTokenRingIndex: params.paidTokenRingIndex,
                    paidTokenRingRevision: params.paidTokenRingRevision,
                    aliasProofs: params.aliasProofs.map { BytesCodable(wrappedValue: $0) }
                ))

            case let .asUnloadTokenFromOutput(params):
                .asUnloadTokenFromOutput(CoinagePallet.AsUnloadTokenFromOutputMode(
                    feeRecyclerValue: params.feeRecyclerValue,
                    feeRecyclerIndex: params.feeRecyclerIndex,
                    feeRecyclerRevision: params.feeRecyclerRevision,
                    retryCounter: params.retryCounter,
                    aliasProofs: params.aliasProofs.map { BytesCodable(wrappedValue: $0) }
                ))

            case let .infallibleUnpaidSigned(nonce):
                .infallibleUnpaidSigned(CoinagePallet.InfallibleUnpaid(nonce: nonce))
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

private extension CoinagePallet.AsCoinageTxExtension {
    func makePeopleMode(
        params: CoinagePallet.AsUnloadTokenPeopleParams,
        implication: TransactionExtension.Implication,
        encodingFactory: DynamicScaleEncodingFactoryProtocol
    ) throws -> CoinagePallet.AsUnloadTokenPeopleMode {
        let payloadFactory = ImplicationSignaturePayloadFactory(formatVersion: extrinsicVersion.formatVersion)
        let implicationData = try payloadFactory.createPayload(from: implication, using: encodingFactory)

        // Alias proofs sign blake2_256(inherited_implication)
        let aliasMessage = try implicationData.blake2b32()

        let aliasProofs = try makeRecyclerAliasProofs(
            message: aliasMessage,
            voucherKeyManagers: params.voucherKeyManagers,
            recyclerProofParams: params.recyclerProofParams
        )

        // People proof signs blake2_256(alias_proofs.encode() ++ inherited_implication)

        let aliasProofsEncoded = try aliasProofs.map(\.wrappedValue).scaleEncoded()
        let peopleMessage = try (aliasProofsEncoded + implicationData).blake2b32()

        let proof = try params.keyManager.createProof(
            peopleMessage,
            members: params.peopleProofParams.ringMembers,
            context: params.unloadToken.unloadTokenContext,
            domainSize: params.peopleProofParams.ringSize
        )
        let proofStruct = CoinagePallet.PeopleProof(
            proof: proof,
            ring: params.peopleRingIndex,
            revision: params.peopleProofParams.revision
        )
        return CoinagePallet.AsUnloadTokenPeopleMode(
            proof: proofStruct,
            period: params.unloadToken.period,
            counter: params.unloadToken.counter,
            aliasProofs: aliasProofs
        )
    }

    func makeRecyclerAliasProofs(
        message: Data,
        voucherKeyManagers: [any BandersnatchKeyManaging],
        recyclerProofParams: MembersProofParams
    ) throws -> [BytesCodable] {
        try voucherKeyManagers.map { keyManager in
            try BytesCodable(
                wrappedValue: keyManager.createProof(
                    message,
                    members: recyclerProofParams.ringMembers,
                    context: UnloadTokenContextBuilder.recyclerAliasContext,
                    domainSize: recyclerProofParams.ringSize
                )
            )
        }
    }
}
