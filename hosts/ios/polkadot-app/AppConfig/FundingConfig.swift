import Foundation
import TrUAPIHost

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

    /// The configured providers, in the published order without repeats. A
    /// name the core does not accept as a product id is skipped and logged,
    /// so one typo does not take the other providers down with it.
    static func providers(in json: [String: Any]?, logger: LoggerProtocol?) -> [ProductId] {
        guard let names = json?[Key.providers] as? [Any] else { return [] }

        var seen = Set<ProductId>()
        return names.compactMap { name in
            guard let name = name as? String else {
                logger?.error("funding_config lists a provider that is not a name: \(name)")
                return nil
            }

            do {
                let productId = try ProductId(name)
                return seen.insert(productId).inserted ? productId : nil
            } catch {
                logger?.error("funding_config lists an unusable provider name \(name): \(error)")
                return nil
            }
        }
    }
}
