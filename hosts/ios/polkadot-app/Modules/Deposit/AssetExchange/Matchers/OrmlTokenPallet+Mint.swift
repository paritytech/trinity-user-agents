import Foundation
import SubstrateSdk

extension OrmlPallet {
    struct SetBalanceCall<C: Codable>: Codable {
        enum CodingKeys: String, CodingKey {
            case who
            case currencyId = "currency_id"
            case newFree = "new_free"
            case newReserve = "new_reserved"
        }

        let who: MultiAddress
        let currencyId: C
        @StringCodable var newFree: Balance
        @StringCodable var newReserve: Balance

        func runtimeCall(for palletName: String) -> RuntimeCall<Self> {
            RuntimeCall(
                moduleName: palletName,
                callName: "set_balance",
                args: self
            )
        }
    }
}
