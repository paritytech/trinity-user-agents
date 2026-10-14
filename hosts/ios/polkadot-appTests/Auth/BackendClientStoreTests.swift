import Testing
import Keystore_iOS
@testable import polkadot_app

@Suite("BackendAuthStore")
struct BackendAuthStoreTests {
    @Test("Returns a wallet on first fetch")
    func returnsWalletOnFirstFetch() throws {
        let store = makeSUT()

        let wallet = try store.fetchAuthWallet()
        let publicKey = try wallet.getRawPublicKey()

        #expect(publicKey.count == 32)
    }

    @Test("Returns same wallet public key on subsequent fetches")
    func returnsStablePublicKey() throws {
        let store = makeSUT()

        let first = try store.fetchAuthWallet().getRawPublicKey()
        let second = try store.fetchAuthWallet().getRawPublicKey()

        #expect(first == second)
    }

    @Test("Different session IDs produce different wallets")
    func differentSessionsDifferentWallets() throws {
        let keychain = InMemoryKeychain()

        let store1 = BackendAuthStore(
            keychain: keychain,
            sessionIdStore: StubSessionIdStore(id: "session-a")
        )
        let store2 = BackendAuthStore(
            keychain: keychain,
            sessionIdStore: StubSessionIdStore(id: "session-b")
        )

        let pubA = try store1.fetchAuthWallet().getRawPublicKey()
        let pubB = try store2.fetchAuthWallet().getRawPublicKey()

        #expect(pubA != pubB)
    }
}

private extension BackendAuthStoreTests {
    func makeSUT() -> BackendAuthStore {
        BackendAuthStore(
            keychain: InMemoryKeychain(),
            sessionIdStore: StubSessionIdStore(id: "test-session")
        )
    }
}

private final class StubSessionIdStore: BackendSessionIdStoring {
    private let id: String

    init(id: String) {
        self.id = id
    }

    func getOrCreateSessionId() -> String { id }
}
