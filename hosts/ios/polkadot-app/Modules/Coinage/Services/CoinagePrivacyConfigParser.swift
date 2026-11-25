import Coinage
import Foundation

enum CoinagePrivacyConfigParser {
    static func parse(_ data: Data?) -> CoinagePrivacyConfig {
        guard
            let data,
            !data.isEmpty,
            let payload = try? JSONDecoder().decode(Payload.self, from: data)
        else {
            return .bundled
        }

        return CoinagePrivacyConfig(
            balanced: thresholds(payload.balanced, fallback: CoinagePrivacyConfig.bundled.balanced),
            maxPrivacy: thresholds(payload.maxPrivacy, fallback: CoinagePrivacyConfig.bundled.maxPrivacy)
        )
    }
}

private extension CoinagePrivacyConfigParser {
    static let percentRange = 0 ... 100
    static let positiveRange = 1 ... Int(UInt32.max)

    struct Payload: Decodable {
        let balanced: Preset?
        let maxPrivacy: Preset?
    }

    struct Preset: Decodable {
        let ringFillPercent: Int?
        let anonymityMinutes: Int?
        let anonymityMembers: Int?
    }

    static func thresholds(
        _ preset: Preset?,
        fallback: CoinagePrivacyConfig.Thresholds
    ) -> CoinagePrivacyConfig.Thresholds {
        guard let preset else { return fallback }

        return CoinagePrivacyConfig.Thresholds(
            ringFillPercent: field(preset.ringFillPercent, in: percentRange, fallback: fallback.ringFillPercent),
            anonymityMinutes: field(preset.anonymityMinutes, in: positiveRange, fallback: fallback.anonymityMinutes),
            anonymityMembers: field(preset.anonymityMembers, in: positiveRange, fallback: fallback.anonymityMembers)
        )
    }

    static func field(_ value: Int?, in range: ClosedRange<Int>, fallback: UInt32) -> UInt32 {
        guard let value, range.contains(value) else { return fallback }

        return UInt32(value)
    }
}
