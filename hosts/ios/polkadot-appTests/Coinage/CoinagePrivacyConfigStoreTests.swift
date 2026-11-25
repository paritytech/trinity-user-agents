import Coinage
import Foundation
import Testing

@testable import polkadot_app

private let fetchedPayload = Data(#"""
{
  "balanced": { "ringFillPercent": 30, "anonymityMinutes": 5, "anonymityMembers": 4 },
  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
}
"""#.utf8)

@Suite("Coinage privacy config store")
struct CoinagePrivacyConfigStoreTests {
    @Test("A fetched config replaces the bundled defaults", .timeLimit(.minutes(1)))
    func fetchedConfigReplacesBundled() async throws {
        // Given
        let configs = StubConfigStream()
        let store = CoinagePrivacyConfigStore(configObserver: configs, logger: StubLogger())
        var iterator = store.privacyConfigStream().makeAsyncIterator()
        let initial = try await iterator.next()
        #expect(initial == CoinagePrivacyConfig.bundled)

        // When
        store.start()
        configs.send(
            RemoteAppConfig(
                identityBackendUrl: nil,
                ipfsGatewayUrl: nil,
                dotNsResolver: nil,
                dotNsNameRegistry: nil,
                coinageInstanceId: nil,
                fundingUrl: nil,
                offrampUrl: nil,
                accountDataStoreContract: nil,
                paymentAsset: nil,
                appSharingUrl: nil,
                coinagePrivacyConfig: fetchedPayload
            )
        )

        // Then
        let fetched = try #require(try await iterator.next())
        #expect(fetched == CoinagePrivacyConfig(
            balanced: .init(ringFillPercent: 30, anonymityMinutes: 5, anonymityMembers: 4),
            maxPrivacy: .init(ringFillPercent: 80, anonymityMinutes: 15, anonymityMembers: 20)
        ))
    }
}
