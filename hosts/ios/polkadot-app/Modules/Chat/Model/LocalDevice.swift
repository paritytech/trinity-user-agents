import Foundation
import Operation_iOS
import SubstrateSdk

extension Chat {
    struct LocalDevice: Equatable {
        let statementAccountId: Data
        let encryptionPublicKey: Data
        let hostName: String
        let createdAt: Date
        let hostVersion: String?
        let osType: String?
        let osVersion: String?
        var outgoingUpdateTime: UInt64?
        var lastSyncOfferId: String?
    }
}

extension Chat.LocalDevice: Identifiable {
    var identifier: String {
        statementAccountId.toHex()
    }

    var displayDeviceName: String {
        let parts = [osType, osVersion].compactMap { $0?.isEmpty == false ? $0 : nil }
        if parts.isEmpty {
            return String(localized: .linkedDevicesDeviceDetailsUnknownDevice)
        }
        return parts.joined(separator: " ")
    }

    var displayHostName: String {
        if let version = hostVersion {
            return "\(hostName) v.\(version)"
        }
        return hostName
    }

    var supportsDeviceSyncSession: Bool {
        hostName == "Polkadot Desktop"
    }
}
