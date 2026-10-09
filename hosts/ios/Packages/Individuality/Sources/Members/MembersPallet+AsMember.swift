import Foundation
import SubstrateSdk
import ExtrinsicService
import KeyDerivation

public extension MembersPallet {
    final class AsMemberTxExtension {
        public let extrinsicVersion: Extrinsic.Version
        public let vrfManager: BandersnatchKeyManaging

        public init(
            extrinsicVersion: Extrinsic.Version,
            vrfManager: BandersnatchKeyManaging
        ) {
            self.extrinsicVersion = extrinsicVersion
            self.vrfManager = vrfManager
        }
    }
}

// MARK: - Encodable Models

public extension MembersPallet.AsMemberTxExtension {
    enum Mode: Encodable {
        case selfInclude(signature: Data)

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()
            switch self {
            case let .selfInclude(signature):
                try container.encode("SelfInclude")
                try container.encode(BytesCodable(wrappedValue: signature))
            }
        }
    }
}

// MARK: - TransactionExtending

extension MembersPallet.AsMemberTxExtension: TransactionExtending {
    public var txExtensionId: String { "AsMember" }

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
        let payloadFactory = ImplicationSignaturePayloadFactory(formatVersion: extrinsicVersion.formatVersion)
        let payload = try payloadFactory.createPayload(from: implication, using: encodingFactory)
        let message = try payload.blake2b32()
        let signature = try vrfManager.sign(message)

        let json = try Mode.selfInclude(signature: signature)
            .toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }
}
