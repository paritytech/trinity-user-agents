import Foundation
import KeyDerivation

extension BandersnatchKeyManager {
    /// Full personhood ring-VRF key: `//peopl.{tld}//index_bytes(0)`.
    static func fullPerson(
        for tld: String,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared
    ) throws -> BandersnatchKeyManager {
        try BandersnatchKeyManager(
            entropyDeriver: RingVrfEntropyDeriver(domain: BuiltInProduct.personhood(for: tld).rawValue, index: 0),
            entropyManager: entropyManager
        )
    }

    /// Light personhood ring-VRF key: `//peopl.{tld}//index_bytes(1)`.
    static func litePerson(
        for tld: String,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared
    ) throws -> BandersnatchKeyManager {
        try BandersnatchKeyManager(
            entropyDeriver: RingVrfEntropyDeriver(domain: BuiltInProduct.personhood(for: tld).rawValue, index: 1),
            entropyManager: entropyManager
        )
    }
}
