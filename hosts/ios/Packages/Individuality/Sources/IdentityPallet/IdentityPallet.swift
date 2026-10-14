import Foundation
import SubstrateSdk

public enum IdentityPallet {
    public static let name = "Identity"

    public struct Registration: Decodable, Equatable {
        public let info: IdentityInformation
    }

    public struct IdentityInformation: Decodable, Equatable {
        public let twitter: ChainData
        public let github: ChainData
        public let discord: ChainData
    }

    public struct Identity: Decodable, Equatable {
        public let info: IdentityInformation

        public static var empty: Identity {
            .init(
                info: .init(
                    twitter: .none,
                    github: .none,
                    discord: .none
                )
            )
        }
    }

    public struct UsernameInfo: Decodable {
        @BytesCodable public var owner: AccountId
    }
}
