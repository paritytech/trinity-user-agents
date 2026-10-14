import Foundation
import Operation_iOS

struct PolkadotSignInHost {
    let accountId: Data
    let publicKey: Data
    let name: String
    let iconUrl: URL?
}

extension PolkadotSignInHost: Operation_iOS.Identifiable {
    var identifier: String {
        accountId.toHex()
    }
}
