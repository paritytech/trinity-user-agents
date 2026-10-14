import SubstrateSdk
import Foundation

struct SearchRow<Payload> {
    let accountId: AccountId
    let username: Username?
    let matchTerms: [String]
    let payload: Payload
}
