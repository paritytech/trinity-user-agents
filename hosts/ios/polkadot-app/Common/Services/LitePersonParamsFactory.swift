import Foundation
import Keystore_iOS
import SubstrateSdk
import NovaCrypto
import MessageExchangeKit
import KeyDerivation
import Individuality

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

/// The messages the three account signatures cover; the runtime prepends its signing context to each.
struct LitePersonSigningMessages {
    let accountIdProof: Data
    let consumerRegistration: Data
    let reservation: Data
}

protocol LitePersonParamsFactoryProtocol {
    func deriveRegistrationParams(
        for usernameBase: String,
        attester: AccountId,
        signedAt: UInt64,
        reservedUsername: String?,
        signingContexts: AccountSigningContexts
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
            ownEncryptionKeyId: ChatEncryptionDomain.mainChat.rawValue
        )
    }
}

private extension LitePersonParamsFactory {
    func encryptionIdentifier() throws -> Chat.OnChainEncryptionIdentifier {
        let chatPublicKey = try Chat.PublicKey(rawData: chatEncryptorFactory.localPublicKey)

        return .x25519(chatPublicKey)
    }

    func accountIdProofMessage() throws -> Data {
        let memberKey = try liteVrfManager.getMemberKey()

        return Data(Self.msgPrefix.utf8) + publicKey.rawData() + memberKey
    }

    func consumerRegistrationMessage(
        for username: String,
        encryptionIdentifier: Chat.OnChainEncryptionIdentifier,
        verifier: AccountId,
        reservedUsername: String?
    ) throws -> Data {
        let encoder = ScaleEncoder()
        encoder.appendRaw(data: publicKey.rawData())
        encoder.appendRaw(data: verifier)
        try encoder.appendRaw(data: encryptionIdentifier.scaleEncoded())
        try username.encode(scaleEncoder: encoder)
        try ScaleOption(value: reservedUsername).encode(scaleEncoder: encoder)

        return encoder.encode()
    }

    func signWithAccount(_ message: Data, signingContext: Data?) throws -> Data {
        let payload = (signingContext ?? Data()) + message

        return try accountIdSigner.sign(payload, context: .rawBytes(publicKey)).rawData()
    }
}

extension LitePersonParamsFactory {
    func signingMessages(
        for usernameBase: String,
        attester: AccountId,
        signedAt: UInt64,
        reservedUsername: String?
    ) throws -> LitePersonSigningMessages {
        let encryptionIdentifier = try encryptionIdentifier()

        let reservation = UsernameReservationMessage(
            candidate: publicKey.rawData(),
            attester: attester,
            usernameBase: usernameBase,
            chatKey: encryptionIdentifier,
            reservedBaseLabel: reservedUsername.map { Data($0.utf8) },
            signedAt: signedAt
        )

        return try LitePersonSigningMessages(
            accountIdProof: accountIdProofMessage(),
            consumerRegistration: consumerRegistrationMessage(
                for: usernameBase,
                encryptionIdentifier: encryptionIdentifier,
                verifier: attester,
                reservedUsername: reservedUsername
            ),
            reservation: reservation.scaleEncoded()
        )
    }
}

extension LitePersonParamsFactory: LitePersonParamsFactoryProtocol {
    func deriveRegistrationParams(
        for usernameBase: String,
        attester: AccountId,
        signedAt: UInt64,
        reservedUsername: String?,
        signingContexts: AccountSigningContexts
    ) throws -> UsernameRegistrationParams {
        let messages = try signingMessages(
            for: usernameBase,
            attester: attester,
            signedAt: signedAt,
            reservedUsername: reservedUsername
        )

        let litePerson = try LitePersonRegistrationParams(
            accountId: publicKey.rawData(),
            accountIdProofSignature: signWithAccount(
                messages.accountIdProof,
                signingContext: signingContexts.peopleLite
            ),
            personMemberKey: liteVrfManager.getMemberKey(),
            // The ring VRF proof is not an account signature, so the runtime checks it over the bare message.
            membershipProofSignature: liteVrfManager.sign(messages.accountIdProof),
            encryptionIdentifier: encryptionIdentifier(),
            username: usernameBase,
            resourcesSignature: signWithAccount(
                messages.consumerRegistration,
                signingContext: signingContexts.peopleLite
            )
        )

        let reservation = try UsernameRegistrationParams.Reservation(
            signature: signWithAccount(messages.reservation, signingContext: signingContexts.dotnsGateway),
            signedAt: signedAt,
            reservedUsername: reservedUsername
        )

        return UsernameRegistrationParams(litePerson: litePerson, reservation: reservation)
    }
}
