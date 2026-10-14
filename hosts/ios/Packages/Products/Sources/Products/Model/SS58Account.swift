import Foundation
import NovaCrypto
import SubstrateSdk

public struct SS58Account: Equatable, Decodable {
    public let accountId: AccountId

    public init(accountId: AccountId) {
        self.accountId = accountId
    }

    public init(from decoder: Decoder) throws {
        let address = try decoder.singleValueContainer().decode(String.self)
        let factory = SS58AddressFactory()
        accountId = try factory.accountId(
            fromAddress: address,
            type: factory.type(fromAddress: address).uint16Value
        )
    }
}
