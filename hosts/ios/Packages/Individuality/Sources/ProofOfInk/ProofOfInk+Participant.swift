import Foundation
import SubstrateSdk

public extension ProofOfInk {
    final class AsParticipantExtension {
        let mode: Mode

        init(mode: Mode) {
            self.mode = mode
        }
    }
}

public extension ProofOfInk.AsParticipantExtension {
    enum Mode: Codable {
        case applyWithSig(AccountNonce)
        case asReferred(AccountNonce)
        case asInvited(AccountNonce)

        public init(from decoder: any Decoder) throws {
            var container = try decoder.unkeyedContainer()

            let type = try container.decode(String.self)

            switch type {
            case "AsApplyWithSig":
                let nonce = try container.decode(StringScaleMapper<AccountNonce>.self).value
                self = .applyWithSig(nonce)
            case "AsReferred":
                let nonce = try container.decode(StringScaleMapper<AccountNonce>.self).value
                self = .asReferred(nonce)
            case "AsInvited":
                let nonce = try container.decode(StringScaleMapper<AccountNonce>.self).value
                self = .asInvited(nonce)
            default:
                throw DecodingError.dataCorruptedError(in: container, debugDescription: "Unsupported mode \(type)")
            }
        }

        public func encode(to encoder: any Encoder) throws {
            var container = encoder.unkeyedContainer()

            switch self {
            case let .applyWithSig(nonce):
                try container.encode("AsApplyWithSig")
                try container.encode(StringScaleMapper<AccountNonce>(value: nonce))
            case let .asReferred(nonce):
                try container.encode("AsReferred")
                try container.encode(StringScaleMapper<AccountNonce>(value: nonce))
            case let .asInvited(nonce):
                try container.encode("AsInvited")
                try container.encode(StringScaleMapper<AccountNonce>(value: nonce))
            }
        }
    }
}

extension ProofOfInk.AsParticipantExtension: OnlyExplicitTransactionExtending {
    public var txExtensionId: String { "AsProofOfInkParticipant" }

    public func explicit(
        for _: TransactionExtension.Implication,
        encodingFactory _: DynamicScaleEncodingFactoryProtocol,
        metadata: RuntimeMetadataProtocol,
        context: RuntimeJsonContext?
    ) throws -> TransactionExtension.Explicit? {
        let json = try mode.toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }
}
