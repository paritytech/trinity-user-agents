import Foundation
import StatementStore
import Keystore_iOS
import NovaCrypto
import KeyDerivation
import Products
import SubstrateSdk

protocol ChatPeerStatementSignerManaging: StatementStoreSignerManaging {
    func makeSigner(with peerAccountId: AccountId) throws -> StatementStoreSigning
}

/// One-shot notification-funded accounts: a fresh account per request (or acceptance) and period,
/// so nothing on chain links it to us or to another request.
protocol ChatRequestSignerManaging {
    func makeChatRequestSigner(for requestId: String, period: UInt32) throws -> StatementStoreSigning
    func makeChatAcceptanceSigner(for requestId: String, period: UInt32) throws -> StatementStoreSigning
}

final class ChatSignerManager {
    let entropyManager: RootEntropyManaging
    let tldProvider: DotNsTldProviding

    init(
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) {
        self.entropyManager = entropyManager
        self.tldProvider = tldProvider
    }
}

extension ChatSignerManager: ChatPeerStatementSignerManaging {
    func makeSigner(for signerKeyId: String) throws -> StatementStoreSigning {
        let keypairFactory = WalletMnemonicKeypairFactory(derivationPath: signerKeyId, entropyManager: entropyManager)

        let keypair = try keypairFactory.deriveKeypair()

        let publicKey = try SNPublicKey(rawData: keypair.publicKey().rawData())
        let privateKey = try SNPrivateKey(rawData: keypair.privateKey().rawData())

        return StatementStoreKeypairSigner(keypair: SNKeypair(privateKey: privateKey, publicKey: publicKey))
    }

    func makeSigner(with peerAccountId: AccountId) throws -> StatementStoreSigning {
        let path = try ChatKeyPaths.statement(tld: tldProvider.currentTldOrError(), peerAccountId: peerAccountId)

        return try makeSigner(for: path)
    }
}

extension ChatSignerManager: ChatRequestSignerManaging {
    func makeChatRequestSigner(for requestId: String, period: UInt32) throws -> StatementStoreSigning {
        try makeSigner(for: ChatKeyPaths.request(requestId: requestId, period: period))
    }

    func makeChatAcceptanceSigner(for requestId: String, period: UInt32) throws -> StatementStoreSigning {
        try makeSigner(for: ChatKeyPaths.accept(requestId: requestId, period: period))
    }
}
