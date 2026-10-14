import Foundation
import SubstrateSdk

struct UsernameSearchResult: Decodable {
    let usernames: [UsernameResponseModel]
    let nextCursor: String?
}

struct UsernameResponseModel: Decodable, Hashable {
    enum Status: String, Decodable {
        case assigned = "ASSIGNED"
        case reserved = "RESERVED"
        case failed = "FAILED"
    }

    enum CodingKeys: String, CodingKey {
        case accountId
        case username
        case createdAt
        case updatedAt
        case status
    }

    let accountId: AccountAddress
    let username: Username
    let status: Status
    let createdAt: String
    let updatedAt: String

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        accountId = try container.decode(AccountAddress.self, forKey: .accountId)
        createdAt = try container.decode(String.self, forKey: .createdAt)
        updatedAt = try container.decode(String.self, forKey: .updatedAt)
        status = try container.decode(Status.self, forKey: .status)
        let name = try container.decode(String.self, forKey: .username)
        username = Username(value: name)
    }
}
