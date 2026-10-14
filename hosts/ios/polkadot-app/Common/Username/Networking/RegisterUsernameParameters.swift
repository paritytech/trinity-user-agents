import Foundation
import SubstrateSdk

struct RegisterUsernameParameters: Encodable {
    struct DotNs: Encodable {
        @HexCodable var signature: Data
        let signedAt: Int
        let reservedUsername: String?
    }

    let username: String
    let preferredDigits: String?
    let candidateAccountId: AccountAddress
    @HexCodable var candidateSignature: Data
    @HexCodable var ringVrfKey: Data
    @HexCodable var proofOfOwnership: Data
    @HexCodable var identifierKey: Data
    @HexCodable var consumerRegistrationSignature: Data
    let dotns: DotNs
}
