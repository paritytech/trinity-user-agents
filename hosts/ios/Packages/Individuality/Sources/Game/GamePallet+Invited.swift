import SubstrateSdk
import Foundation

public extension GamePallet {
    final class GameAsInvitedExtension: Codable {
        @StringCodable public var nonce: AccountNonce
        @BytesCodable public var inviter: AccountId
        @BytesCodable public var ticket: Data
        public let signature: MultiSignature

        public init(
            nonce: AccountNonce,
            inviter: AccountId,
            ticket: AccountId,
            signature: MultiSignature
        ) {
            self.nonce = nonce
            self.inviter = inviter
            self.ticket = ticket
            self.signature = signature
        }
    }
}

extension GamePallet.GameAsInvitedExtension: OnlyExplicitTransactionExtending {
    public var txExtensionId: String { "GameAsInvited" }

    public func explicit(
        for _: TransactionExtension.Implication,
        encodingFactory _: DynamicScaleEncodingFactoryProtocol,
        metadata: RuntimeMetadataProtocol,
        context: RuntimeJsonContext?
    ) throws -> TransactionExtension.Explicit? {
        let json = try toScaleCompatibleJSON(with: context?.toRawContext())

        return TransactionExtension.Explicit(
            from: json,
            txExtensionId: txExtensionId,
            metadata: metadata
        )
    }
}
