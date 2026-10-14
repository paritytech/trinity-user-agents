import Foundation
import SubstrateSdk

// MARK: - RequestDeviceInfo

/// Per-device encrypted symmetric key entry.
/// Each entry holds the statement store account ID of a target device
/// and the one-shot symmetric key encrypted with that device's P-256 public key.
public struct RequestDeviceInfo: Equatable {
    public let statementAccountId: Data
    public let encryptedKey: Data

    public init(
        statementAccountId: Data,
        encryptedKey: Data
    ) {
        self.statementAccountId = statementAccountId
        self.encryptedKey = encryptedKey
    }
}

extension RequestDeviceInfo: ScaleCodable {
    static let statementAccountIdLength = 32

    public init(scaleDecoder: any ScaleDecoding) throws {
        statementAccountId = try scaleDecoder.readAndConfirm(count: Self.statementAccountIdLength)
        encryptedKey = try Data(scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        scaleEncoder.appendRaw(data: statementAccountId)
        try encryptedKey.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - MultiDeviceRequest

/// Multi-device request envelope.
/// `encryptedRequest` contains a `Request<M>` encrypted with a one-shot ChaCha20-Poly1305 key (REQ_PK).
/// `devicesInfo` contains REQ_PK encrypted individually for each recipient device.
public struct MultiDeviceRequest: Equatable {
    public let encryptedRequest: Data
    public let devicesInfo: [RequestDeviceInfo]

    public init(
        encryptedRequest: Data,
        devicesInfo: [RequestDeviceInfo]
    ) {
        self.encryptedRequest = encryptedRequest
        self.devicesInfo = devicesInfo
    }
}

extension MultiDeviceRequest: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        encryptedRequest = try Data(scaleDecoder: scaleDecoder)
        devicesInfo = try [RequestDeviceInfo](scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try encryptedRequest.encode(scaleEncoder: scaleEncoder)
        try devicesInfo.encode(scaleEncoder: scaleEncoder)
    }
}

// MARK: - MultiDeviceResponse

/// Multi-device response envelope.
/// `encryptedResponse` contains a `Response` encrypted with a one-shot ChaCha20-Poly1305 key (RES_PK).
/// `devicesInfo` contains RES_PK encrypted individually for each recipient device.
public struct MultiDeviceResponse: Equatable {
    public let encryptedResponse: Data
    public let devicesInfo: [RequestDeviceInfo]

    public init(
        encryptedResponse: Data,
        devicesInfo: [RequestDeviceInfo]
    ) {
        self.encryptedResponse = encryptedResponse
        self.devicesInfo = devicesInfo
    }
}

extension MultiDeviceResponse: ScaleCodable {
    public init(scaleDecoder: any ScaleDecoding) throws {
        encryptedResponse = try Data(scaleDecoder: scaleDecoder)
        devicesInfo = try [RequestDeviceInfo](scaleDecoder: scaleDecoder)
    }

    public func encode(scaleEncoder: any ScaleEncoding) throws {
        try encryptedResponse.encode(scaleEncoder: scaleEncoder)
        try devicesInfo.encode(scaleEncoder: scaleEncoder)
    }
}
