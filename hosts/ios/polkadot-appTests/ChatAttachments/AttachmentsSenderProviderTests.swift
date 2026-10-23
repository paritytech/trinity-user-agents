import Foundation
import Testing
@testable import polkadot_app

struct AttachmentsSenderProviderTests {
    private let entropyManager = FixedRootEntropyManager()

    @Test func uploadsFromSharedChatHopAccount() async throws {
        let wallet = try await makeProvider().getWallet()

        #expect(try wallet.getRawPublicKey() == account(at: ChatKeyPaths.hop(tld: "dot")))
    }
}

private extension AttachmentsSenderProviderTests {
    func makeProvider() -> AttachmentsSenderProvider {
        AttachmentsSenderProvider(
            entropyManager: entropyManager,
            tldProvider: StubDotNsTldProvider(tld: "dot")
        )
    }

    func account(at path: String) throws -> Data {
        try ChatSignerManager(entropyManager: entropyManager).makeSigner(for: path).accountId
    }
}
