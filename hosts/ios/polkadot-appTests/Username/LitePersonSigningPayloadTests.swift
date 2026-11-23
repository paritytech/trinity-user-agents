import Foundation
import Testing
import NovaCrypto
import SubstrateSdk
import KeyDerivation
import Individuality
@testable import polkadot_app

/// The runtimes check the account signatures sent during onboarding over `context ++ message`
/// once they publish a signing context, and over the bare message until then.
struct LitePersonSigningPayloadTests {
    static let contexts: [AccountSigningContexts] = [
        AccountSigningContexts(peopleLite: nil, dotnsGateway: nil),
        AccountSigningContexts(
            peopleLite: Data(repeating: 0x11, count: 32),
            dotnsGateway: Data(repeating: 0x22, count: 32)
        )
    ]

    let attester = Data(repeating: 0x33, count: 32)
    let usernameBase = "alice"
    let signedAt: UInt64 = 1_700_000_000

    @Test(arguments: contexts)
    func accountSignaturesCoverContextThenMessage(contexts: AccountSigningContexts) throws {
        let entropyManager = try FixedRootEntropyManager(entropy: Data.randomOrError(of: 32))
        let factory = try LitePersonParamsFactory(
            mainWallet: DynamicDerivedWallet(derivationPath: "//wallet", entropyManager: entropyManager),
            liteVrfManager: MockBandersnatchKeyManager(),
            chatEncryptorManager: ChatEncryptionManager(entropyManager: entropyManager)
        )

        let params = try factory.deriveRegistrationParams(
            for: usernameBase,
            attester: attester,
            signedAt: signedAt,
            reservedUsername: usernameBase,
            signingContexts: contexts
        )
        let messages = try factory.signingMessages(
            for: usernameBase,
            attester: attester,
            signedAt: signedAt,
            reservedUsername: usernameBase
        )

        let verifier = AccountSignatureVerifier(accountId: params.litePerson.accountId)
        let peopleLite = contexts.peopleLite ?? Data()
        let dotnsGateway = contexts.dotnsGateway ?? Data()

        #expect(try verifier.verifies(
            params.litePerson.accountIdProofSignature,
            over: peopleLite + messages.accountIdProof
        ))
        #expect(try verifier.verifies(
            params.litePerson.resourcesSignature,
            over: peopleLite + messages.consumerRegistration
        ))
        #expect(try verifier.verifies(
            params.reservation.signature,
            over: dotnsGateway + messages.reservation
        ))
    }
}

private struct AccountSignatureVerifier {
    let accountId: AccountId

    func verifies(_ signature: Data, over payload: Data) throws -> Bool {
        try SNSignatureVerifier().verify(
            SNSignature(rawData: signature),
            forOriginalData: payload,
            using: SNPublicKey(rawData: accountId)
        )
    }
}
