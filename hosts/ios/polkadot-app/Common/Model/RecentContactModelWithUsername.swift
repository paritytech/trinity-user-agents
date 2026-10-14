import Foundation
import Operation_iOS
import ChainRegistry

struct RecentContactModelWithUsername: Identifiable {
    var identifier: String {
        recentContact.identifier
    }

    let recentContact: RecentContactModel
    let username: Username?
    let chainAsset: ChainAsset?
}
