import Foundation
import Testing
@testable import polkadot_app

/// The app's note of which sessions had their cancel offer accepted.
struct FundingAcceptedCancelsTests {
    private func freshDefaults() throws -> UserDefaults {
        let name = "FundingAcceptedCancelsTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: name))
        defaults.removePersistentDomain(forName: name)
        return defaults
    }

    @Test
    func anAcceptedCancelIsRemembered() throws {
        let defaults = try freshDefaults()

        FundingAcceptedCancels.remember("intent-1", in: defaults)

        #expect(FundingAcceptedCancels.contains("intent-1", in: defaults))
        #expect(!FundingAcceptedCancels.contains("intent-2", in: defaults))
    }

    @Test
    func onlyTheLatestSessionsAreKept() throws {
        let defaults = try freshDefaults()

        for index in 0 ..< 70 {
            FundingAcceptedCancels.remember("intent-\(index)", in: defaults)
        }

        #expect(!FundingAcceptedCancels.contains("intent-0", in: defaults))
        #expect(FundingAcceptedCancels.contains("intent-69", in: defaults))
    }
}
