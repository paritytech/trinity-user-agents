import Foundation

/// The parts of the `funding_config` remote object the iOS host reads:
/// `{ "providers": ["ramp.dot", …] }`, the dotNS names of the funding
/// providers the overlay offers. The core offers only the providers the host
/// lists, so a build with none configured (and none bundled) offers none.
enum FundingConfig {
    enum Key {
        static let providers = "providers"
    }

    /// The configured provider names, in the published order without
    /// repeats. An entry that is not a name is skipped and logged, so one
    /// typo does not take the other providers down with it.
    static func providers(in json: [String: Any]?, logger: LoggerProtocol?) -> [String] {
        guard let names = json?[Key.providers] as? [Any] else { return [] }

        var seen = Set<String>()
        return names.compactMap { name in
            guard let name = name as? String, !name.isEmpty else {
                logger?.error("funding_config lists a provider that is not a name: \(name)")
                return nil
            }

            return seen.insert(name).inserted ? name : nil
        }
    }
}
