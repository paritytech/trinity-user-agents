import Foundation
import SubstrateSdk

public extension IdentityPallet {
    struct PersonIdentity: Decodable, Equatable {
        public enum Social: Codable, Equatable {
            case twitter(SocialUsername)
            case github(SocialUsername)
            case discord(SocialDisplayAndTag)
            case unsupported(String)

            public init(from decoder: Decoder) throws {
                var unkeyedContainer = try decoder.unkeyedContainer()
                let type = try unkeyedContainer.decode(String.self)

                switch type {
                case "Twitter":
                    self = try .twitter(unkeyedContainer.decode(SocialUsername.self))
                case "Github":
                    self = try .github(unkeyedContainer.decode(SocialUsername.self))
                case "Discord":
                    self = try .discord(unkeyedContainer.decode(SocialDisplayAndTag.self))
                default:
                    self = .unsupported(type)
                }
            }

            public func encode(to encoder: any Encoder) throws {
                var unkeyedContainer = encoder.unkeyedContainer()

                switch self {
                case let .twitter(username):
                    try unkeyedContainer.encode("Twitter")
                    try unkeyedContainer.encode(username)
                case let .github(username):
                    try unkeyedContainer.encode("Github")
                    try unkeyedContainer.encode(username)
                case let .discord(displayAndTag):
                    try unkeyedContainer.encode("Discord")
                    try unkeyedContainer.encode(displayAndTag)
                case let .unsupported(type):
                    try unkeyedContainer.encode(type)
                }
            }
        }

        public struct SocialUsername: Codable, Equatable {
            @BytesCodable public var username: Data
        }

        public struct SocialDisplayAndTag: Codable, Equatable {
            @BytesCodable public var displayAndTag: Data
        }

        public struct PendingJudgement: Decodable, Equatable {
            public let social: Social
            public let judgementId: JSON

            public init(from decoder: Decoder) throws {
                var unkeyedContainer = try decoder.unkeyedContainer()

                social = try unkeyedContainer.decode(Social.self)
                judgementId = try unkeyedContainer.decode(JSON.self)
            }
        }

        @BytesCodable public var account: AccountId
        public let pendingJudgements: [PendingJudgement]
        public let banned: Bool

        public static var empty: PersonIdentity {
            .init(
                account: AccountId.zeroAccountId(of: 32),
                pendingJudgements: [],
                banned: false
            )
        }
    }
}
