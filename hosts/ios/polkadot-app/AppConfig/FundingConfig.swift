import Foundation

/// The parts of the `funding_config` remote object the iOS host reads:
/// `{ "onrampUrl": …, "offrampUrl": …, "providers": ["ramp.dot", …] }`, the
/// CASH card's destinations and the dotNS names of the funding providers the
/// overlay offers. The core offers only the providers the host lists, so a
/// build with none configured (and none bundled) offers none.
enum FundingConfig {
    enum Key {
        static let providers = "providers"
    }

    /// The text field `field`, or nil when it is missing, empty or not text.
    /// The object mixes text with the `providers` list, so it is read field
    /// by field rather than as text throughout.
    static func string(_ field: String, in json: [String: Any]?) -> String? {
        guard let value = json?[field] as? String, !value.isEmpty else { return nil }
        return value
    }

    /// The configured provider names, in the published order. An entry that
    /// is not text is skipped and logged, so one typo does not take the other
    /// providers down with it; the core drops repeats and names that are not
    /// product ids.
    static func providers(in json: [String: Any]?, logger: LoggerProtocol?) -> [String] {
        guard let names = json?[Key.providers] as? [Any] else { return [] }

        return names.compactMap { name in
            guard let name = name as? String, !name.isEmpty else {
                logger?.error("funding_config lists a provider that is not a name: \(name)")
                return nil
            }

            return name
        }
    }
}
