import Foundation
import NovaCrypto
import SubstrateSdk

public enum KeystoreTag {
    public static var domain: String {
        "io.polkadotapp"
    }

    static func rootEntropyTag(for entropyId: String) -> String {
        [
            domain,
            entropyId,
            "root.entropy"
        ]
        .joined(separator: ":")
    }
}
