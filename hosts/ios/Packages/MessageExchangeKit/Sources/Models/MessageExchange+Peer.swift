import Foundation

public extension MessageExchange {
    /// Describes a single device belonging to a peer contact.
    struct DeviceInfo: Hashable {
        /// The sr25519 account ID this device uses for statement store signing.
        public let statementAccountId: Data

        /// The P-256 public key this device uses for encryption.
        public let encryptionPublicKey: Data

        public init(statementAccountId: Data, encryptionPublicKey: Data) {
            self.statementAccountId = statementAccountId
            self.encryptionPublicKey = encryptionPublicKey
        }
    }

    struct Peer: Hashable {
        public let accountId: AccountId
        public let publicKey: Data
        public let pin: String?

        /// Known devices for this peer. When non-empty, multi-device messaging is used.
        public let devices: [DeviceInfo]

        public init(
            accountId: AccountId,
            publicKey: Data,
            pin: String?,
            devices: [DeviceInfo]
        ) {
            self.accountId = accountId
            self.publicKey = publicKey
            self.pin = pin
            self.devices = devices
        }
    }

    struct Own: Hashable {
        public let signKeyId: String
        public let encryptionKeyId: String
        public let pin: String?

        public init(signKeyId: String, encryptionKeyId: String, pin: String?) {
            self.signKeyId = signKeyId
            self.encryptionKeyId = encryptionKeyId
            self.pin = pin
        }
    }
}
