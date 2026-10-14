import Testing
import Foundation
@testable import polkadot_app

@Suite("SystemLocaleProvider")
struct SystemLocaleProviderTests {
    @Test("emits the current language tag on subscribe")
    func emitsCurrentTag() async throws {
        let provider = SystemLocaleProvider()

        var iterator = provider.subscribe().makeAsyncIterator()

        let tag = try #require(await iterator.next())
        #expect(tag == SystemLocaleProvider.currentLanguageTag())
    }
}
