import Foundation
import KeyDerivation

extension BandersnatchKeyManager {
    /// Full personhood ring-VRF key: `//peopl.{tld}//index_bytes(0)`.
    static func fullPerson(
        for tld: String,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared
    ) -> BandersnatchKeyManager {
        BandersnatchKeyManager(
            entropyDeriver: RingVrfEntropyDeriver(domain: BuiltInProduct.personhood(for: tld), index: 0),
            entropyManager: entropyManager
        )
    }

    /// Light personhood ring-VRF key: `//peopl.{tld}//index_bytes(1)`.
    static func litePerson(
        for tld: String,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared
    ) -> BandersnatchKeyManager {
        BandersnatchKeyManager(
            entropyDeriver: RingVrfEntropyDeriver(domain: BuiltInProduct.personhood(for: tld), index: 1),
            entropyManager: entropyManager
        )
    }
}
