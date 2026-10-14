import Foundation
import Keystore_iOS

extension Keychain {
    convenience init() {
        self.init(accessLevel: .always)
    }
}
