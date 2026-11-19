import Foundation
import StatementStore
import Testing
@testable import MessageExchangeKit

struct PeerSessionPriorityProviderTests {
    let provider = PeerSessionPriorityProvider(priorityFactory: FixedPriorityFactory(), logger: nil)

    @Test func seedIsTheNextExpiryAfterTheLatestStatement() {
        #expect(provider.initialExpiry(from: [[.expiry(500)], [.expiry(300)]]) == 501)
    }

    @Test func seedSaturatesInsteadOfOverflowing() {
        #expect(provider.initialExpiry(from: [[.expiry(.max)]]) == .max)
    }

    @Test func incrementSaturatesInsteadOfTrapping() {
        provider.expiry = .max

        #expect(provider.incrementedExpiry() == .max)
    }
}
