import Foundation
import SubstrateSdk

struct UsernameMetadata {
    let minLength: Int
    let maxLength: Int

    static let `default` = UsernameMetadata(minLength: 6, maxLength: 30)
}

struct UsernameAttester: Decodable {
    @HexCodable var attester: AccountId
}

struct UsernameResponse: Decodable {
    enum CodingKeys: String, CodingKey {
        case baseUsername = "base_username"
        case digits
        case username
    }

    let baseUsername: String
    let digits: String
    let username: String
}
