import Foundation

/// The parts of the `funding_config` remote object the iOS host reads:
/// `{ "providers": ["ramp.dot", …] }`, the dotNS names of the funding
/// providers the overlay offers. The core offers only the providers the host
/// lists, so a build with none configured offers none.
enum FundingConfig {
    enum Key {
        static let providers = "providers"
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
