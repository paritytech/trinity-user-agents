import Foundation
import BigInt
import SubstrateSdk

extension BalancesPallet {
    struct Transfer: Codable {
        let dest: MultiAddress
        @StringCodable var value: BigUInt
    }

    struct TransferAll: Codable {
        enum CodingKeys: String, CodingKey {
            case dest
            case keepAlive = "keep_alive"
        }

        let dest: MultiAddress
        let keepAlive: Bool

        static var codingPath: CallCodingPath {
            .init(moduleName: BalancesPallet.name, callName: "transfer_all")
        }
    }

    static var transferAllowDeathCallPath: CallCodingPath {
        .init(moduleName: BalancesPallet.name, callName: "transfer_allow_death")
    }

    static var transferCallPath: CallCodingPath {
        .init(moduleName: BalancesPallet.name, callName: "transfer")
    }
}
