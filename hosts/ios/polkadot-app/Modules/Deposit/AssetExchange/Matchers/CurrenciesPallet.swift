import Foundation
import SubstrateSdk

enum CurrenciesPallet {
    static let moduleName = "Currencies"

    static var depositedEventPath: EventCodingPath {
        .init(moduleName: moduleName, eventName: "Deposited")
    }
}
