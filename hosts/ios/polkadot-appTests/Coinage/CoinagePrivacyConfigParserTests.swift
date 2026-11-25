import Coinage
import Foundation
import Testing

@testable import polkadot_app

private let validMaxPrivacy = CoinagePrivacyConfig.Thresholds(
    ringFillPercent: 80,
    anonymityMinutes: 15,
    anonymityMembers: 20
)

private func expectedConfig(balanced: CoinagePrivacyConfig.Thresholds) -> CoinagePrivacyConfig {
    CoinagePrivacyConfig(balanced: balanced, maxPrivacy: validMaxPrivacy)
}

@Suite("Coinage privacy config parser")
struct CoinagePrivacyConfigParserTests {
    struct ParseCase: Sendable, CustomTestStringConvertible {
        let label: String
        let json: String?
        let expected: CoinagePrivacyConfig

        var testDescription: String { label }
    }

    @Test(
        "Falls back to bundled values only for the parts of the payload that are unusable",
        arguments: [
            ParseCase(
                label: "nil payload",
                json: nil,
                expected: .bundled
            ),
            ParseCase(
                label: "empty data",
                json: "",
                expected: .bundled
            ),
            ParseCase(
                label: "full valid payload",
                json: #"""
                {
                  "balanced": { "ringFillPercent": 30, "anonymityMinutes": 5, "anonymityMembers": 4 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: .init(ringFillPercent: 30, anonymityMinutes: 5, anonymityMembers: 4))
            ),
            ParseCase(
                label: "balanced object missing",
                json: #"""
                {
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: CoinagePrivacyConfig.bundled.balanced)
            ),
            ParseCase(
                label: "one field missing",
                json: #"""
                {
                  "balanced": { "ringFillPercent": 30, "anonymityMinutes": 5 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: .init(ringFillPercent: 30, anonymityMinutes: 5, anonymityMembers: 8))
            ),
            ParseCase(
                label: "ringFillPercent above 100",
                json: #"""
                {
                  "balanced": { "ringFillPercent": 101, "anonymityMinutes": 5, "anonymityMembers": 4 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: .init(ringFillPercent: 20, anonymityMinutes: 5, anonymityMembers: 4))
            ),
            ParseCase(
                label: "anonymityMinutes of zero",
                json: #"""
                {
                  "balanced": { "ringFillPercent": 30, "anonymityMinutes": 0, "anonymityMembers": 4 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: .init(
                    ringFillPercent: 30,
                    anonymityMinutes: 10,
                    anonymityMembers: 4
                ))
            ),
            ParseCase(
                label: "negative anonymityMembers",
                json: #"""
                {
                  "balanced": { "ringFillPercent": 30, "anonymityMinutes": 5, "anonymityMembers": -3 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: .init(ringFillPercent: 30, anonymityMinutes: 5, anonymityMembers: 8))
            ),
            ParseCase(
                label: "ringFillPercent of zero is valid",
                json: #"""
                {
                  "balanced": { "ringFillPercent": 0, "anonymityMinutes": 5, "anonymityMembers": 4 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: .init(ringFillPercent: 0, anonymityMinutes: 5, anonymityMembers: 4))
            ),
            ParseCase(
                label: "anonymityMembers above UInt32.max",
                json: #"""
                {
                  "balanced": { "ringFillPercent": 30, "anonymityMinutes": 5, "anonymityMembers": 4294967296 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: expectedConfig(balanced: .init(ringFillPercent: 30, anonymityMinutes: 5, anonymityMembers: 8))
            ),
            ParseCase(
                label: "string ringFillPercent discards whole payload",
                json: #"""
                {
                  "balanced": { "ringFillPercent": "20", "anonymityMinutes": 5, "anonymityMembers": 4 },
                  "maxPrivacy": { "ringFillPercent": 80, "anonymityMinutes": 15, "anonymityMembers": 20 }
                }
                """#,
                expected: .bundled
            ),
            ParseCase(
                label: "malformed JSON",
                json: "{",
                expected: .bundled
            )
        ]
    )
    func parsesPayloadWithPerFieldFallback(_ testCase: ParseCase) {
        // Given
        let data = testCase.json.map { Data($0.utf8) }

        // When
        let config = CoinagePrivacyConfigParser.parse(data)

        // Then
        #expect(config == testCase.expected)
    }
}
