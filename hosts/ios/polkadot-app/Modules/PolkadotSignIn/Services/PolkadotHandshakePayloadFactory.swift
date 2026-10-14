import Foundation
import KeyDerivation
import MessageExchangeKit
import SubstrateSdk
import CryptoKit

protocol PolkadotHandshakePayloadMaking {
    func makeSuccessPayload(
        hostData: HandshakeProposal,
        deviceData: HandshakeDeviceData,
        rootAccountId: Data,
        identityAccountId: Data
    ) throws -> Data

    func makeV2StatusPayload(
        response: EncryptedHandshakeResponseV2,
        deviceData: HandshakeDeviceData
    ) throws -> Data
}

final class PolkadotHandshakePayloadFactory {
    private let chatEncryptorFactory: MessageExchangeEncryptionMaking
    private let ssoEncryptorFactory: MessageExchangeEncryptionMaking
    private let deviceEncryptionKeyManager: DeviceEncryptionKeyManaging
    private let rootEntropySourceDeriver: any RootEntropySourceDeriving

    init(
        chatEncryptorFactory: MessageExchangeEncryptionMaking,
        ssoEncryptorFactory: MessageExchangeEncryptionMaking,
        deviceEncryptionKeyManager: DeviceEncryptionKeyManaging,
        rootEntropySourceDeriver: any RootEntropySourceDeriving
    ) {
        self.chatEncryptorFactory = chatEncryptorFactory
        self.ssoEncryptorFactory = ssoEncryptorFactory
        self.deviceEncryptionKeyManager = deviceEncryptionKeyManager
        self.rootEntropySourceDeriver = rootEntropySourceDeriver
    }
}

extension PolkadotHandshakePayloadFactory: PolkadotHandshakePayloadMaking {
    func makeSuccessPayload(
        hostData: HandshakeProposal,
        deviceData: HandshakeDeviceData,
        rootAccountId: Data,
        identityAccountId: Data
    ) throws -> Data {
        switch hostData {
        case .v1:
            let sensitiveData = HandshakeSuccessV1(
                sharedSecretDerivationKey: chatEncryptorFactory.localPublicKey,
                rootUserAccountId: rootAccountId,
                identityAccountId: identityAccountId
            )
            return try makeEncryptedV1Payload(
                sensitiveData: sensitiveData,
                deviceData: deviceData
            )
        case .v2:
            let successData = try HandshakeSuccessV2(
                identityAccountId: identityAccountId,
                rootAccountId: rootAccountId,
                identityChatPrivateKey: chatEncryptorFactory.localPrivateKey,
                ssoEncrPubKey: ssoEncryptorFactory.localPublicKey,
                deviceEncPubKey: deviceEncryptionKeyManager.getPublicKey(),
                rootEntropySource: rootEntropySourceDeriver.deriveRootEntropySource()
            )
            let response = EncryptedHandshakeResponseV2.success(successData)
            return try makeEncryptedV2Payload(
                response: response,
                deviceData: deviceData
            )
        }
    }

    func makeV2StatusPayload(
        response: EncryptedHandshakeResponseV2,
        deviceData: HandshakeDeviceData
    ) throws -> Data {
        try makeEncryptedV2Payload(
            response: response,
            deviceData: deviceData
        )
    }
}

private extension PolkadotHandshakePayloadFactory {
    func makeEncryptor(
        deviceData: HandshakeDeviceData
    ) throws -> (encryptor: MessageExchangeEncrypting, publicKey: Data) {
        let tempPrivateKey = Curve25519.KeyAgreement.PrivateKey()
        let tempEncryptorFactory = X25519ChaChaPolyEncryptorFactory(privateKey: tempPrivateKey)
        let encryptor = try tempEncryptorFactory.makeEncryptor(
            remotePublicKey: deviceData.encryptionPublicKey
        )
        return (encryptor, tempEncryptorFactory.localPublicKey)
    }

    func makeEncryptedV1Payload(
        sensitiveData: HandshakeSuccessV1,
        deviceData: HandshakeDeviceData
    ) throws -> Data {
        let (encryptor, publicKey) = try makeEncryptor(deviceData: deviceData)
        let encryptedMessage = try encryptor.encrypt(sensitiveData.scaleEncoded())
        let handshakeData = HandshakeResponse.v1(.init(
            encryptedMessage: encryptedMessage,
            publicKey: publicKey
        ))
        let rawData = try handshakeData.scaleEncoded()
        return try rawData.scaleEncoded()
    }

    func makeEncryptedV2Payload(
        response: EncryptedHandshakeResponseV2,
        deviceData: HandshakeDeviceData
    ) throws -> Data {
        let (encryptor, publicKey) = try makeEncryptor(deviceData: deviceData)
        let encryptedMessage = try encryptor.encrypt(response.scaleEncoded())
        let handshakeData = HandshakeResponse.v2(.init(
            encryptedMessage: encryptedMessage,
            publicKey: publicKey
        ))
        let rawData = try handshakeData.scaleEncoded()
        return try rawData.scaleEncoded()
    }
}
