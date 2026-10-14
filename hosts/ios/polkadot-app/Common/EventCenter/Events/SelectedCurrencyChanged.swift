import Foundation
import ChainRegistry
import EventCenter

struct SelectedCurrencyChanged: EventProtocol {
    func accept(visitor: EventVisitorProtocol) {
        (visitor as? AppEventVisiting)?.processSelectedCurrencyChanged(event: self)
    }
}
