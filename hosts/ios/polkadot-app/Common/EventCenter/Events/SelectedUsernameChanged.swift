import Foundation
import ChainRegistry
import EventCenter

struct SelectedUsernameChanged: EventProtocol {
    let username: Username?

    func accept(visitor: EventVisitorProtocol) {
        (visitor as? AppEventVisiting)?.processSelectedUsernameChanged(event: self)
    }
}
