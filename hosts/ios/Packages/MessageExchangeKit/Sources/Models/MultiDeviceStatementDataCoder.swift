import Foundation
import CryptoKit
import SubstrateSdk
import SDKLogger

// MARK: - Device Info for encryption

/// Describes a recipient device for multi-device encryption.
public struct RecipientDeviceInfo {
    public let statementAccountId: Data
    public let encryptionPublicKey: Data

    public init(
        statementAccountId: Data,
        encryptionPublicKey: Data
    ) {
        self.statementAccountId = statementAccountId
        self.encryptionPublicKey = encryptionPublicKey
    }
}

// MARK: - Errors

public enum MultiDeviceEncodingError: Error {
    case payloadEncodingFailed
    case payloadEncryptionFailed
    case deviceKeyEncryptionFailed
}

public enum MultiDeviceDecodingError: Error {
    case deviceEntryNotFound
    case oneshotKeyDecryptionFailed
    case payloadDecryptionFailed
    case payloadDecodingFailed
}

// MARK: - Protocol

public protocol MultiDeviceStatementDataCoding {
    func encodeMultiDeviceRequest(
        _ request: MessageExchange.Request<some MessageExchange.CodableMessage>,
        recipients: [RecipientDeviceInfo]
    ) throws -> MultiDeviceRequest

    func encodeMultiDeviceResponse(
        _ response: MessageExchange.Response,
        recipients: [RecipientDeviceInfo]
    ) throws -> MultiDeviceResponse

    func decodeMultiDeviceRequest<M: MessageExchange.CodableMessage>(
        _ multiRequest: MultiDeviceRequest,
        ownStatementAccountId: Data,
        peerDevicePublicKey: Data
    ) throws -> MessageExchange.Request<M>

    func decodeMultiDeviceResponse(
        _ multiResponse: MultiDeviceResponse,
        ownStatementAccountId: Data,
        peerDevicePublicKey: Data
    ) throws -> MessageExchange.Response
}

// MARK: - Implementation

public final class MultiDeviceStatementDataCoder: MultiDeviceStatementDataCoding {
    private let deviceEncryptionKeyFactory: MessageExchangeEncryptionMaking
    private let logger: SDKLoggerProtocol?

    public init(
        deviceEncryptionKeyFactory: MessageExchangeEncryptionMaking,
        logger: SDKLoggerProtocol?
    ) {
        self.deviceEncryptionKeyFactory = deviceEncryptionKeyFactory
        self.logger = logger
    }

    // MARK: - Encoding

    public func encodeMultiDeviceRequest(
        _ request: MessageExchange.Request<some MessageExchange.CodableMessage>,
        recipients: [RecipientDeviceInfo]
    ) throws -> MultiDeviceRequest {
        let requestData: Data

        do {
            requestData = try request.scaleEncoded()
        } catch {
            logger?.error("Failed to encode request: \(error)")
            throw MultiDeviceEncodingError.payloadEncodingFailed
        }

        let (encryptedPayload, devicesInfo) = try encryptForDevices(
            payload: requestData,
            recipients: recipients
        )

        return MultiDeviceRequest(
            encryptedRequest: encryptedPayload,
            devicesInfo: devicesInfo
        )
    }

    public func encodeMultiDeviceResponse(
        _ response: MessageExchange.Response,
        recipients: [RecipientDeviceInfo]
    ) throws -> MultiDeviceResponse {
        let responseData: Data

        do {
            responseData = try response.scaleEncoded()
        } catch {
            logger?.error("Failed to encode response: \(error)")
            throw MultiDeviceEncodingError.payloadEncodingFailed
        }

        let (encryptedPayload, devicesInfo) = try encryptForDevices(
            payload: responseData,
            recipients: recipients
        )

        return MultiDeviceResponse(
            encryptedResponse: encryptedPayload,
            devicesInfo: devicesInfo
        )
    }

    // MARK: - Decoding

    public func decodeMultiDeviceRequest<M: MessageExchange.CodableMessage>(
        _ multiRequest: MultiDeviceRequest,
        ownStatementAccountId: Data,
        peerDevicePublicKey: Data
    ) throws -> MessageExchange.Request<M> {
        let decryptedData = try decryptFromDevice(
            encryptedPayload: multiRequest.encryptedRequest,
            devicesInfo: multiRequest.devicesInfo,
            ownStatementAccountId: ownStatementAccountId,
            peerDevicePublicKey: peerDevicePublicKey
        )

        do {
            let decoder = try ScaleDecoder(data: decryptedData)
            return try MessageExchange.Request<M>(scaleDecoder: decoder)
        } catch {
            logger?.error("Failed to decode inner request: \(error)")
            throw MultiDeviceDecodingError.payloadDecodingFailed
        }
    }

    public func decodeMultiDeviceResponse(
        _ multiResponse: MultiDeviceResponse,
        ownStatementAccountId: Data,
        peerDevicePublicKey: Data
    ) throws -> MessageExchange.Response {
        let decryptedData = try decryptFromDevice(
            encryptedPayload: multiResponse.encryptedResponse,
            devicesInfo: multiResponse.devicesInfo,
            ownStatementAccountId: ownStatementAccountId,
            peerDevicePublicKey: peerDevicePublicKey
        )

        do {
            let decoder = try ScaleDecoder(data: decryptedData)
            return try MessageExchange.Response(scaleDecoder: decoder)
        } catch {
            logger?.error("Failed to decode inner response: \(error)")
            throw MultiDeviceDecodingError.payloadDecodingFailed
        }
    }
}

// MARK: - Private Helpers

private extension MultiDeviceStatementDataCoder {
    /// Generates a one-shot ChaCha20-Poly1305 key, encrypts the payload with it,
    /// then encrypts the key for each recipient device using X25519 + ChaCha20-Poly1305.
    func encryptForDevices(
        payload: Data,
        recipients: [RecipientDeviceInfo]
    ) throws -> (encryptedPayload: Data, devicesInfo: [RequestDeviceInfo]) {
        let oneshotKey = SymmetricKey(size: .bits256)
        let encryptedPayload: Data

        do {
            encryptedPayload = try ChaChaPoly.seal(payload, using: oneshotKey).combined
        } catch {
            logger?.error("Failed to encrypt payload with oneshot key: \(error)")
            throw MultiDeviceEncodingError.payloadEncryptionFailed
        }

        let oneshotKeyData = oneshotKey.withUnsafeBytes { Data($0) }

        var devicesInfo: [RequestDeviceInfo] = []
        devicesInfo.reserveCapacity(recipients.count)

        for recipient in recipients {
            do {
                let encryptor = try deviceEncryptionKeyFactory.makeEncryptor(
                    remotePublicKey: recipient.encryptionPublicKey
                )
                let encryptedKey = try encryptor.encrypt(oneshotKeyData)

                devicesInfo.append(RequestDeviceInfo(
                    statementAccountId: recipient.statementAccountId,
                    encryptedKey: encryptedKey
                ))
            } catch {
                logger?.error("Failed to encrypt key for device: \(error)")
                throw MultiDeviceEncodingError.deviceKeyEncryptionFailed
            }
        }

        return (encryptedPayload, devicesInfo)
    }

    /// Finds the device entry matching ownStatementAccountId, decrypts the one-shot key
    /// using the sender device's public key, and uses it to decrypt the inner payload.
    func decryptFromDevice(
        encryptedPayload: Data,
        devicesInfo: [RequestDeviceInfo],
        ownStatementAccountId: Data,
        peerDevicePublicKey: Data
    ) throws -> Data {
        guard let deviceEntry = devicesInfo.first(
            where: { $0.statementAccountId == ownStatementAccountId }
        ) else {
            logger?.error("No device entry found for own account ID")
            throw MultiDeviceDecodingError.deviceEntryNotFound
        }

        let oneshotKeyData: Data

        do {
            let encryptor = try deviceEncryptionKeyFactory.makeEncryptor(
                remotePublicKey: peerDevicePublicKey
            )
            oneshotKeyData = try encryptor.decrypt(deviceEntry.encryptedKey)
        } catch {
            logger?.error("Failed to decrypt oneshot key with sender device key: \(error)")
            throw MultiDeviceDecodingError.oneshotKeyDecryptionFailed
        }

        let oneshotKey = SymmetricKey(data: oneshotKeyData)

        do {
            let box = try ChaChaPoly.SealedBox(combined: encryptedPayload)
            return try ChaChaPoly.open(box, using: oneshotKey)
        } catch {
            logger?.error("Failed to decrypt payload with oneshot key: \(error)")
            throw MultiDeviceDecodingError.payloadDecryptionFailed
        }
    }
}
