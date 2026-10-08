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

        static func reserve() -> URL {
            URL(string: DeepLink.scheme + "://tattoo")!
        }

        static func tattooUploading() -> URL {
            URL(string: DeepLink.scheme + "://tattoo/uploading")!
        }

        static func game(intendedGameIndex: Int? = nil) -> URL {
            var components = URLComponents()
            components.scheme = DeepLink.scheme
            components.host = "game"

            if let intendedGameIndex {
                components.queryItems = [
                    URLQueryItem(name: PushNotificationKeys.gameIndex, value: "\(intendedGameIndex)")
                ]
            }

            return components.url!
        }

        static func players(game: UInt32, gameDate: Date) -> URL {
            let idPart = "id=\(game)"
            let datePart = "date=\(gameDate.formatted(.iso8601))"

            return URL(string: DeepLink.scheme + "://players?\(idPart)&\(datePart)")!
        }
    }
}
