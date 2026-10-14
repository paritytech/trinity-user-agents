import Foundation
import Keystore_iOS
import SubstrateSdk
import NovaCrypto
import MessageExchangeKit
import KeyDerivation

struct LitePersonRegistrationParams {
    let accountId: AccountId
    let accountIdProofSignature: Data
    let personMemberKey: BandersnatchPubKey
    let membershipProofSignature: Data
    let encryptionIdentifier: Chat.OnChainEncryptionIdentifier
    let username: String
    let resourcesSignature: Data
}

struct UsernameRegistrationParams {
    struct Reservation {
        let signature: Data
        let signedAt: UInt64
        let reservedUsername: String?
    }

    let litePerson: LitePersonRegistrationParams
    let reservation: Reservation
}

protocol LitePersonParamsFactoryProtocol {
    func deriveRegistrationParams(
        for usernameBase: String,
        attester: AccountId,
        signedAt: UInt64,
        reservedUsername: String?
    ) throws -> UsernameRegistrationParams
}

final class LitePersonParamsFactory {
    static let msgPrefix = "pop:people-lite:register using"

    let publicKey: SNPublicKey
    let accountIdSigner: SigningWrapperProtocol
    let liteVrfManager: BandersnatchKeyManaging
    let chatEncryptorFactory: MessageExchangeEncryptionMaking

    init(
        mainWallet: WalletManaging,
        liteVrfManager: BandersnatchKeyManaging,
        chatEncryptorManager: MessageExchangeEncryptionManaging
    ) throws {
        publicKey = try SNPublicKey(rawData: mainWallet.getRawPublicKey())

        accountIdSigner = DefaultSigningWrapper(secretProvider: mainWallet)

        self.liteVrfManager = liteVrfManager

        chatEncryptorFactory = try chatEncryptorManager.makeEncryptorFactory(
            ownEncryptionKeyId: Chat.Contact.Own.main().encryptionKeyId
        )
    }
}

private extension LitePersonParamsFactory {
    func prepareLitePersonSignatureData() throws -> Data {
        let memberKey = try liteVrfManager.getMemberKey()

        return Data(Self.msgPrefix.utf8) + publicKey.rawData() + memberKey
    }

    func prepareResourcesSignatureData(
        for username: String,
        encryptionIdentifier: Chat.OnChainEncryptionIdentifier,
        verifier: AccountId,
        reservedUsername: String?
    ) throws -> Data {
        let resourcesSignatureDataCoder = ScaleEncoder()
        resourcesSignatureDataCoder.appendRaw(data: publicKey.rawData())
        resourcesSignatureDataCoder.appendRaw(data: verifier)
        try resourcesSignatureDataCoder.appendRaw(data: encryptionIdentifier.scaleEncoded())
        try username.encode(scaleEncoder: resourcesSignatureDataCoder)

        try ScaleOption(value: reservedUsername).encode(
            scaleEncoder: resourcesSignatureDataCoder
        )

        return resourcesSignatureDataCoder.encode()
    }
}

extension LitePersonParamsFactory {
    func deriveLitePersonParams(
        for username: String,
        verifier: AccountId,
        reservedUsername: String?
    ) throws -> LitePersonRegistrationParams {
        let litePersonSignatureData = try prepareLitePersonSignatureData()

        let accountIdProofSignature = try accountIdSigner.sign(
            litePersonSignatureData,
            context: .rawBytes(publicKey)
        )
        .rawData()

        let personSignature = try liteVrfManager.sign(litePersonSignatureData)

        let chatPublicKey = try Chat.PublicKey(rawData: chatEncryptorFactory.localPublicKey)
        let encryptionIdentifier = Chat.OnChainEncryptionIdentifier.x25519(chatPublicKey)

        let resourcesSignatureData = try prepareResourcesSignatureData(
            for: username,
            encryptionIdentifier: encryptionIdentifier,
            verifier: verifier,
            reservedUsername: reservedUsername
        )

        let resourcesSignature = try accountIdSigner.sign(resourcesSignatureData, context: .rawBytes(publicKey))

        let memberKey = try liteVrfManager.getMemberKey()

        return LitePersonRegistrationParams(
            accountId: publicKey.rawData(),
            accountIdProofSignature: accountIdProofSignature,
            personMemberKey: memberKey,
            membershipProofSignature: personSignature,
            encryptionIdentifier: encryptionIdentifier,
            username: username,
            resourcesSignature: resourcesSignature.rawData()
        )
    }
}

extension LitePersonParamsFactory: LitePersonParamsFactoryProtocol {
    func deriveRegistrationParams(
        for usernameBase: String,
        attester: AccountId,
        signedAt: UInt64,
        reservedUsername: String?
    ) throws -> UsernameRegistrationParams {
        let liteParams = try deriveLitePersonParams(
            for: usernameBase,
            verifier: attester,
            reservedUsername: reservedUsername
        )

        let message = UsernameReservationMessage(
            candidate: publicKey.rawData(),
            attester: attester,
            usernameBase: usernameBase,
            chatKey: liteParams.encryptionIdentifier,
            reservedBaseLabel: reservedUsername.map { Data($0.utf8) },
            signedAt: signedAt
        )

        let reservationSignature = try accountIdSigner.sign(
            message.scaleEncoded(),
            context: .rawBytes(publicKey)
        )

        return UsernameRegistrationParams(
            litePerson: liteParams,
            reservation: UsernameRegistrationParams.Reservation(
                signature: reservationSignature.rawData(),
                signedAt: signedAt,
                reservedUsername: reservedUsername
            )
        )
    }
}
