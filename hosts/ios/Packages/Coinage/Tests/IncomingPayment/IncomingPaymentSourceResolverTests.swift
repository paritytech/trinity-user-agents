import Testing
import Foundation
import NovaCrypto
@testable import Coinage

/// `accept` validates a source by resolving it, so a coins source must be refused here rather than
/// stored and failed later as notClaimed.
struct IncomingPaymentSourceResolverTests {
    private static let validSecret = Data(repeating: 0x07, count: 64)

    private func makeResolver() -> IncomingPaymentSourceResolver {
        IncomingPaymentSourceResolver(
            entropyManager: MockEntropyManager(entropy: Data(repeating: 0x02, count: 32)),
            snKeyFactory: SNKeyFactory()
        )
    }

    @Test func emptyCoinsAreRefused() async {
        await #expect(throws: IncomingPaymentSourceResolverError.emptyCoinKeys) {
            _ = try await makeResolver().resolve(descriptor: .coins(secretKeys: []))
        }
    }

    @Test func coinKeyThatDerivesNoPublicKeyIsRefused() async {
        await #expect(throws: IncomingPaymentSourceResolverError.invalidCoinKey) {
            _ = try await makeResolver().resolve(descriptor: .coins(secretKeys: [Self.validSecret, Data([0x01])]))
        }
    }

    @Test func validCoinKeysResolveUnchanged() async throws {
        let resolved = try await makeResolver().resolve(descriptor: .coins(secretKeys: [Self.validSecret]))
        guard case let .coins(secretKeys) = resolved else {
            Issue.record("expected a coins source")
            return
        }
        #expect(secretKeys == [Self.validSecret])
    }
}
