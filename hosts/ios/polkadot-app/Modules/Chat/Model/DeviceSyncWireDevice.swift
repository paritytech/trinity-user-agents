import Foundation
import SubstrateSdk

// MARK: - Model

extension Chat {
    enum DeviceSyncDeviceStatus: UInt8, Equatable {
        case active = 0
    }

    struct DeviceSyncWireDevice: Equatable {
        static let accountIdLength = 32
        static let encryptionKeyLength = 32

        let statementAccountId: Data
        let encryptionPublicKey: Data
        let status: DeviceSyncDeviceStatus
        let lastUpdate: UInt64
    }
}

// MARK: - Local Mapping

extension Chat.DeviceSyncWireDevice {
    init(from local: Chat.LocalDevice) {
        statementAccountId = local.statementAccountId
        encryptionPublicKey = local.encryptionPublicKey
        status = .active
        lastUpdate = UInt64(local.createdAt.timeIntervalSince1970 * 1_000)
    }
}

// MARK: - ScaleCodable

extension Chat.DeviceSyncDeviceStatus: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        let index = try UInt8(scaleDecoder: scaleDecoder)
        guard let value = Self(rawValue: index) else {
            throw ScaleCodingError.unexpectedDecodedValue
        }
        self = value
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        try rawValue.encode(scaleEncoder: scaleEncoder)
    }
}

extension Chat.DeviceSyncWireDevice: ScaleCodable {
    init(scaleDecoder: any ScaleDecoding) throws {
        statementAccountId = try scaleDecoder.readAndConfirm(count: Self.accountIdLength)
        encryptionPublicKey = try scaleDecoder.readAndConfirm(count: Self.encryptionKeyLength)
        status = try Chat.DeviceSyncDeviceStatus(scaleDecoder: scaleDecoder)
        lastUpdate = try UInt64(scaleDecoder: scaleDecoder)
    }

    func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: statementAccountId)
        scaleEncoder.appendRaw(data: encryptionPublicKey)
        try status.encode(scaleEncoder: scaleEncoder)
        try lastUpdate.encode(scaleEncoder: scaleEncoder)
    }
}
