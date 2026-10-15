import Foundation

extension AppConfig {
    enum DeepLink {
        static var scheme: String { Brand.deeplinkScheme }

        /// Every build flavor's scheme is the brand base plus a per-configuration suffix, so
        /// a prefix test recognises links minted by any flavor without enumerating them.
        static func isKnownScheme(_ scheme: String) -> Bool {
            scheme.lowercased().hasPrefix(Brand.deeplinkBase)
        }

        static func chat(_ chatId: Chat.Id, force: Bool) -> URL {
            let idPart = "id=\(chatId.rawRepresentation)"
            let forcePart = "force=\(force)"

            return URL(string: DeepLink.scheme + "://chat?\(idPart)&\(forcePart)")!
        }

        static func fiatOnramp(sessionId: String) -> URL {
            URL(string: DeepLink.scheme + "://fiatOnramp/buySuccess?sessionId=\(sessionId)")!
        }
    }
}
