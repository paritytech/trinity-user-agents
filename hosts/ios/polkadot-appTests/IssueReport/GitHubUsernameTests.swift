import Testing
@testable import polkadot_app

struct GitHubUsernameTests {
    @Test func onlyGitHubLoginsAreAcceptedBecauseTheNameIsPostedUnescapedAsAMention() {
        let expected: [String: String?] = [
            " @octo-cat ": "octo-cat",
            String(repeating: "a", count: 39): String(repeating: "a", count: 39),
            String(repeating: "a", count: 40): nil,
            "-octocat": nil,
            "octocat-": nil,
            "octo--cat": nil,
            "octo_cat": nil,
            "octocat\n## Logs": nil,
            "@@octocat": nil,
            "": nil
        ]

        #expect(Dictionary(uniqueKeysWithValues: expected.keys.map { ($0, GitHubUsername($0)?.value) }) == expected)
    }
}
