import Foundation
import SubstrateSdk

struct SearchAccountResult {
    struct Contact {
        let username: String?
        let address: AccountAddress
    }

    let recent: [RecentContactModelWithUsername]
    let contacts: [Contact]
    let global: AccountSearchGlobal<Contact>

    init(
        recent: [RecentContactModelWithUsername],
        contacts: [Contact],
        global: AccountSearchGlobal<Contact> = .loaded([])
    ) {
        self.recent = recent
        self.contacts = contacts
        self.global = global
    }
}
