import Foundation
import SubstrateSdk
import BigInt

extension BalancesPallet {
    static var balancesDeposit: EventCodingPath {
        EventCodingPath(moduleName: name, eventName: "Deposit")
    }

    static var balancesMinted: EventCodingPath {
        EventCodingPath(moduleName: name, eventName: "Minted")
    }
}

extension BalancesPallet {
    struct DepositEvent: Decodable {
        let accountId: AccountId
        let amount: BigUInt

        init(from decoder: Decoder) throws {
            var unkeyedContainer = try decoder.unkeyedContainer()

            accountId = try unkeyedContainer.decode(BytesCodable.self).wrappedValue
            amount = try unkeyedContainer.decode(StringScaleMapper<BigUInt>.self).value
        }
    }

    struct MintedEvent: Decodable {
        let accountId: AccountId
        let amount: BigUInt

        init(from decoder: Decoder) throws {
            var unkeyedContainer = try decoder.unkeyedContainer()

            accountId = try unkeyedContainer.decode(BytesCodable.self).wrappedValue
            amount = try unkeyedContainer.decode(StringScaleMapper<BigUInt>.self).value
        }
    }
}

extension BalancesPallet {
    struct ForceSetBalance: Codable {
        enum CodingKeys: String, CodingKey {
            case who
            case newFree = "new_free"
        }

        let who: MultiAddress
        @StringCodable var newFree: Balance

        func runtimeCall() -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: BalancesPallet.name,
                callName: "force_set_balance",
                args: self
            )
        }
    }
}
