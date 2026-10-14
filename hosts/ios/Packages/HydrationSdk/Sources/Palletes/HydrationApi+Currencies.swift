import Foundation
import SubstrateSdk
import SubstrateStateCall

public extension HydrationApi {
    static var currenciesAccountPath: StateCallPath {
        StateCallPath(module: "CurrenciesApi", method: "account")
    }

    struct CurrencyData: Decodable {
        @StringCodable public var free: Balance
        @StringCodable public var reserved: Balance
        @StringCodable public var frozen: Balance
    }
}
