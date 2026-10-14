import Foundation
@testable import polkadot_app

final class MockJWTTokenStore: JWTTokenStoring, @unchecked Sendable {
    private let mutex = NSLock()
    private var storedToken: String?
    private var storedRefreshToken: String?

    private(set) var saveCallCount = 0
    private(set) var fetchCallCount = 0
    private(set) var deleteCallCount = 0

    func saveToken(_ token: String) throws {
        mutex.withLock {
            storedToken = token
            saveCallCount += 1
        }
    }

    func fetchToken() -> String? {
        mutex.withLock {
            fetchCallCount += 1
            return storedToken
        }
    }

    func deleteToken() {
        mutex.withLock {
            storedToken = nil
            deleteCallCount += 1
        }
    }

    func saveRefreshToken(_ token: String) throws {
        mutex.withLock { storedRefreshToken = token }
    }

    func fetchRefreshToken() -> String? {
        mutex.withLock { storedRefreshToken }
    }

    func deleteRefreshToken() {
        mutex.withLock { storedRefreshToken = nil }
    }

    func deleteAll() {
        mutex.withLock {
            storedToken = nil
            storedRefreshToken = nil
            deleteCallCount += 1
        }
    }
}
