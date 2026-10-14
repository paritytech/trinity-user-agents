import Testing
import Keystore_iOS
@testable import polkadot_app

@Suite("JWTTokenStore")
struct JWTTokenStoreTests {
    let keychain: InMemoryKeychain
    let store: JWTTokenStore

    init() {
        keychain = InMemoryKeychain()
        store = JWTTokenStore(keychain: keychain, sessionIdStore: StubSessionIdStore())
    }

    // MARK: - Access token

    @Test("Returns nil when no token stored")
    func fetchReturnsNilWhenEmpty() {
        #expect(store.fetchToken() == nil)
    }

    @Test("Saves and fetches token")
    func saveAndFetchRoundTrip() throws {
        try store.saveToken("test-jwt-token")

        let fetched = store.fetchToken()
        #expect(fetched == "test-jwt-token")
    }

    @Test("Overwrites existing token")
    func overwritesToken() throws {
        try store.saveToken("first-token")
        try store.saveToken("second-token")

        #expect(store.fetchToken() == "second-token")
    }

    @Test("Delete clears stored token")
    func deleteToken() throws {
        try store.saveToken("token-to-delete")
        store.deleteToken()

        #expect(store.fetchToken() == nil)
    }

    @Test("Delete on empty store does not throw")
    func deleteOnEmptyStore() {
        store.deleteToken()
        #expect(store.fetchToken() == nil)
    }

    // MARK: - Refresh token

    @Test("Returns nil when no refresh token stored")
    func fetchRefreshTokenReturnsNilWhenEmpty() {
        #expect(store.fetchRefreshToken() == nil)
    }

    @Test("Saves and fetches refresh token")
    func saveAndFetchRefreshToken() throws {
        try store.saveRefreshToken("refresh-token-abc")

        #expect(store.fetchRefreshToken() == "refresh-token-abc")
    }

    @Test("Overwrites existing refresh token")
    func overwritesRefreshToken() throws {
        try store.saveRefreshToken("first-refresh")
        try store.saveRefreshToken("second-refresh")

        #expect(store.fetchRefreshToken() == "second-refresh")
    }

    @Test("Delete clears stored refresh token")
    func deleteRefreshToken() throws {
        try store.saveRefreshToken("refresh-to-delete")
        store.deleteRefreshToken()

        #expect(store.fetchRefreshToken() == nil)
    }

    // MARK: - Token and refresh token are independent

    @Test("Access and refresh tokens are stored independently")
    func tokensAreIndependent() throws {
        try store.saveToken("access-token")
        try store.saveRefreshToken("refresh-token")

        store.deleteToken()

        #expect(store.fetchToken() == nil)
        #expect(store.fetchRefreshToken() == "refresh-token")
    }

    @Test("Deleting refresh token does not affect access token")
    func deleteRefreshDoesNotAffectAccess() throws {
        try store.saveToken("access-token")
        try store.saveRefreshToken("refresh-token")

        store.deleteRefreshToken()

        #expect(store.fetchToken() == "access-token")
        #expect(store.fetchRefreshToken() == nil)
    }

    // MARK: - Delete all

    @Test("deleteAll clears both tokens")
    func deleteAllClearsBoth() throws {
        try store.saveToken("access-token")
        try store.saveRefreshToken("refresh-token")

        store.deleteAll()

        #expect(store.fetchToken() == nil)
        #expect(store.fetchRefreshToken() == nil)
    }

    @Test("deleteAll on empty store does not throw")
    func deleteAllOnEmptyStore() {
        store.deleteAll()
        #expect(store.fetchToken() == nil)
        #expect(store.fetchRefreshToken() == nil)
    }
}

private final class StubSessionIdStore: BackendSessionIdStoring {
    func getOrCreateSessionId() -> String { "test-session-id" }
}
