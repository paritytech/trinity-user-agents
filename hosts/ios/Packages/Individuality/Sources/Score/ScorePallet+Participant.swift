import SubstrateSdk

public extension ScorePallet {
    struct Participant: Decodable, Equatable {
        @StringCodable public var score: UInt32
        public let streak: Streak
        @StringCodable public var credit: Balance
        public let cashedOut: Bool
        public let recognition: Recognition
        public let reachedPersonhood: Bool
    }
}

public extension ScorePallet {
    final class ScoreAsParticipantExtension: Codable {
        @StringCodable public var nonce: AccountNonce

        public init(nonce: AccountNonce) {
            self.nonce = nonce
        }
    }
}

extension ScorePallet.ScoreAsParticipantExtension: OnlyExplicitTransactionExtending {
    public var txExtensionId: String { "ScoreAsParticipant" }

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
