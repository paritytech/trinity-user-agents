import Foundation
import StatementStore
import SubstrateSdk

// MARK: - Statement Store

public struct StatementsSubscribeDto: Codable {
    public struct Filter: Codable {
        public let matchAll: [HexCodable<Data>]?
        public let matchAny: [HexCodable<Data>]?
    }

    public let filter: Filter

    public func toStatementStoreFilter() throws -> TopicFilter {
        if let matchAll = filter.matchAll {
            let topics = try matchAll.map { try $0.wrappedValue.fixedStatementFieldData() }
            return .matchAll(topics)
        }

        if let matchAny = filter.matchAny {
            let topics = try matchAny.map { try $0.wrappedValue.fixedStatementFieldData() }
            return .matchAny(topics)
        }

        return .matchAll([])
    }
}

public struct StatementProofDto: Codable {
    public let tag: String
    @HexCodable public var signature: Data
    @HexCodable public var signer: Data

    public init(tag: String, signature: Data, signer: Data) {
        self.tag = tag
        _signature = HexCodable(wrappedValue: signature)
        _signer = HexCodable(wrappedValue: signer)
    }
}

public extension StatementProofDto {
    enum DtoError: Error {
        case unsupportedTag
    }
}

public extension StatementProofDto {
    init(proof: StatementProof) {
        switch proof {
        case let .sr25519(signature, signer):
            tag = "Sr25519"
            _signature = HexCodable(wrappedValue: signature)
            _signer = HexCodable(wrappedValue: signer)
        }
    }

    func toStatementProof() throws -> StatementProof {
        switch tag {
        case "Sr25519":
            return .sr25519(signature: signature, signer: signer)
        default:
            throw DtoError.unsupportedTag
        }
    }
}

public struct CreateStatementProofDto: Codable {
    public let account: ProductAccountId
    @OptionHexCodable public var channel: Data?
    @OptionStringCodable public var expiry: UInt64?
    public let topics: [HexCodable<Data>]
    @OptionHexCodable public var data: Data?

    public func toUnsignedRemoteStatement() throws -> Statement {
        try Statement.fromStatementFields(
            topics: topics.map(\.wrappedValue),
            channel: channel,
            expiry: expiry,
            data: data,
            proof: nil
        )
    }
}

public struct CreateStatementProofAuthorizedDto: Codable {
    @OptionHexCodable public var channel: Data?
    @OptionStringCodable public var expiry: UInt64?
    public let topics: [HexCodable<Data>]
    @OptionHexCodable public var data: Data?

    public func toUnsignedRemoteStatement() throws -> Statement {
        try Statement.fromStatementFields(
            topics: topics.map(\.wrappedValue),
            channel: channel,
            expiry: expiry ?? 0xFFFF_FFFF_0000_0000,
            data: data,
            proof: nil
        )
    }
}

public struct StatementsPageDto {
    public let statements: [StatementDto]
    public let isComplete: Bool

    public init(statements: [StatementDto], isComplete: Bool) {
        self.statements = statements
        self.isComplete = isComplete
    }
}

public struct StatementDto: Codable {
    enum DtoError: Error {
        case missingProof
    }

    public let proof: StatementProofDto
    @OptionHexCodable public var channel: Data?
    @OptionStringCodable public var expiry: UInt64?
    public let topics: [HexCodable<Data>]
    @OptionHexCodable public var data: Data?

    public func toRemoteStatement() throws -> Statement {
        try Statement.fromStatementFields(
            topics: topics.map(\.wrappedValue),
            channel: channel,
            expiry: expiry ?? 0xFFFF_FFFF_0000_0000,
            data: data,
            proof: proof.toStatementProof()
        )
    }

    public init(remoteStatement: Statement) throws {
        guard let remoteProof = remoteStatement.getProof() else {
            throw DtoError.missingProof
        }

        proof = StatementProofDto(proof: remoteProof)

        var topics: [HexCodable<Data>] = []

        if let topic1 = remoteStatement.getTopic1() {
            topics.append(HexCodable(wrappedValue: topic1))
        }

        if let topic2 = remoteStatement.getTopic2() {
            topics.append(HexCodable(wrappedValue: topic2))
        }

        if let topic3 = remoteStatement.getTopic3() {
            topics.append(HexCodable(wrappedValue: topic3))
        }

        if let topic4 = remoteStatement.getTopic4() {
            topics.append(HexCodable(wrappedValue: topic4))
        }

        self.topics = topics
        _channel = OptionHexCodable(wrappedValue: remoteStatement.getChannel())
        expiry = remoteStatement.getExpiry()

        if let scaledEncodedData = remoteStatement.getScaleEncodedPayload() {
            let decodedData = try Data(scaleDecoder: ScaleDecoder(data: scaledEncodedData))
            _data = OptionHexCodable(wrappedValue: decodedData)
        } else {
            _data = OptionHexCodable(wrappedValue: nil)
        }
    }
}
