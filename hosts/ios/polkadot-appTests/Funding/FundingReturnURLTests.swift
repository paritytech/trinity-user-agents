import Foundation
import Testing
@testable import polkadot_app

/// When a provider page counts as having reached its return URL, and the
/// origin its frame's bar shows.
struct FundingReturnURLTests {
    private let returnURL = URL(string: "https://provider.example/funding/done")!

    @Test
    func matchesWhateverTheQueryOrFragment() throws {
        let reached = try #require(URL(string: "https://provider.example/funding/done?status=ok&id=7#top"))

        #expect(FundingReturnURL.matches(reached, returnURL: returnURL))
    }

    @Test
    func matchesTheSchemeAndHostInAnyCase() throws {
        let reached = try #require(URL(string: "HTTPS://Provider.Example/funding/done"))

        #expect(FundingReturnURL.matches(reached, returnURL: returnURL))
    }

    @Test
    func matchesTheDefaultPortWrittenOut() throws {
        let reached = try #require(URL(string: "https://provider.example:443/funding/done"))

        #expect(FundingReturnURL.matches(reached, returnURL: returnURL))
    }

    @Test
    func doesNotMatchAnotherPath() throws {
        let reached = try #require(URL(string: "https://provider.example/funding/done/more"))

        #expect(!FundingReturnURL.matches(reached, returnURL: returnURL))
    }

    @Test
    func doesNotMatchAnotherHost() throws {
        let reached = try #require(URL(string: "https://provider.example.evil/funding/done"))

        #expect(!FundingReturnURL.matches(reached, returnURL: returnURL))
    }

    @Test
    func doesNotMatchAnotherScheme() throws {
        let reached = try #require(URL(string: "http://provider.example/funding/done"))

        #expect(!FundingReturnURL.matches(reached, returnURL: returnURL))
    }

    @Test
    func doesNotMatchAnotherPort() throws {
        let reached = try #require(URL(string: "https://provider.example:8443/funding/done"))

        #expect(!FundingReturnURL.matches(reached, returnURL: returnURL))
    }

    @Test
    func readsAnEmptyPathAsTheRoot() throws {
        let root = try #require(URL(string: "https://provider.example/"))
        let reached = try #require(URL(string: "https://provider.example?done=1"))

        #expect(FundingReturnURL.matches(reached, returnURL: root))
    }

    @Test
    func showsTheHostWithoutTheDefaultPort() throws {
        let page = try #require(URL(string: "https://widget.provider.example:443/buy?amount=10"))

        #expect(FundingReturnURL.displayOrigin(of: page) == "widget.provider.example")
    }

    @Test
    func showsAPortThatIsNotTheDefault() throws {
        let page = try #require(URL(string: "https://widget.provider.example:8443/buy"))

        #expect(FundingReturnURL.displayOrigin(of: page) == "widget.provider.example:8443")
    }
}
