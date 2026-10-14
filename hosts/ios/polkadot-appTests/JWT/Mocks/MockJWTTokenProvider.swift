import Foundation
@testable import polkadot_app

final class MockJWTTokenProvider: JWTTokenProviding, @unchecked Sendable {
    private let mutex = NSLock()

    var tokenToReturn: String = "mock-jwt-token"
    var shouldThrowOnValidToken: Error?
    private(set) var invalidateCallCount = 0
    private(set) var validTokenCallCount = 0

    func validToken() async throws -> String {
        mutex.withLock { validTokenCallCount += 1 }

        if let error = shouldThrowOnValidToken {
            throw error
        }

        return tokenToReturn
    }

    func invalidateToken() {
        mutex.withLock { invalidateCallCount += 1 }
    }

    func prewarm() {}

    func withAuthorizedToken<R>(
        _ operation: @escaping (String) async throws -> R
    ) async throws -> R {
        let token = try await validToken()
        return try await operation(token)
    }
}
