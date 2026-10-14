import Foundation
import Operation_iOS
import SubstrateSdk

extension Chat {
    struct RemovedChat: Equatable {
        let accountId: AccountId
        let removedAt: Date
    }
}

extension Chat.RemovedChat: Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
