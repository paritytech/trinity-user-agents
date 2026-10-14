import Foundation
import SubstrateSdk

public struct SubmittedData {
    public let hash: Data
    public let poolStatus: PoolStatus

    public init(hash: Data, poolStatus: PoolStatus) {
        self.hash = hash
        self.poolStatus = poolStatus
    }
}

struct SubmissionModel: Encodable {
    enum CodingKeys: String, CodingKey {
        case data
        case recipients
        case signature
        case signer
        case submitTimestamp = "submit_timestamp"
    }

    @HexCodable var data: Data
    let recipients: [HexCodable<AccountId>]
    @HexCodable var signature: Data
    @HexCodable var signer: Data
    let submitTimestamp: UInt64
}

struct ClaimModel: Encodable {
    enum CodingKeys: String, CodingKey {
        case hash = "raw_hash"
        case signature
    }

    @HexCodable var hash: Data
    @HexCodable var signature: Data
}

struct AckModel: Encodable {
    enum CodingKeys: String, CodingKey {
        case hash = "raw_hash"
        case signature
    }

    @HexCodable var hash: Data
    @HexCodable var signature: Data
}

public struct PoolStatus: Decodable {
    public let entryCount: Int
    public let totalBytes: Int
    public let maxBytes: Int

    public init(entryCount: Int, totalBytes: Int, maxBytes: Int) {
        self.entryCount = entryCount
        self.totalBytes = totalBytes
        self.maxBytes = maxBytes
    }
}

struct SubmitResult: Decodable {
    var poolStatus: PoolStatus
}

enum RpcContext {
    static let submit = Data("hop-submit-v1:".utf8)
    static let ack = Data("hop-ack-v1:".utf8)
    static let claim = Data("hop-claim-v1:".utf8)
}
