import Foundation
import SubstrateSdk

extension Chat {
    struct RemoteContact {
        let accountId: AccountId
        let username: String
        let chatPublicKey: Chat.PublicKey
        let imageData: Data?
    }
}
