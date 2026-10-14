import Foundation
import SubstrateSdk

public struct LegacyAccountId: Hashable {
    public static let length = 32

    public let accountId: AccountId

    public init(accountId: AccountId) {
        self.accountId = accountId
    }
}

extension LegacyAccountId: Decodable {
    public init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let hex = try container.decode(String.self)
        accountId = try Data(hexString: hex)
    }
}

extension LegacyAccountId: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        accountId = try scaleDecoder.readAndConfirm(count: Self.length)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: accountId)
    }
}
