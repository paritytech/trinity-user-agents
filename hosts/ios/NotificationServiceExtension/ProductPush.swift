import Foundation
import Keystore_iOS

/// The room of the chat product that a push id names, with the key of its notifications. The app keeps the routes
/// the chat product gives; the notification service extension reads them to open a push.
struct ProductPushRoute: Codable, Equatable {
    let productId: String
    let roomId: String
    let key: Data
    /// The contact's name, which the notification shows as its title.
    var chatTitle: String?
}

/// The push routes of product rooms, in the keychain that the app and its notification service extension share.
struct ProductPushRoutes {
    private let keychain: KeystoreProtocol

    init(keychain: KeystoreProtocol = Keychain()) {
        self.keychain = keychain
    }

    /// The route of `pushId`, or nil when no product room of this device has it.
    func route(pushId: String) -> ProductPushRoute? {
        guard let data = try? keychain.fetchKey(for: Self.routeTag(pushId)) else { return nil }
        return try? JSONDecoder().decode(ProductPushRoute.self, from: data)
    }

    /// Replaces the chat routes of `productId`: routes not listed are forgotten.
    func replaceChatRoutes(productId: String, routes: [String: ProductPushRoute]) throws {
        let indexTag = Self.chatIndexTag(productId: productId)
        let previous = (try? keychain.fetchKey(for: indexTag))
            .flatMap { try? JSONDecoder().decode([String].self, from: $0) } ?? []
        for pushId in previous where routes[pushId] == nil {
            try? keychain.deleteKey(for: Self.routeTag(pushId))
        }
        for (pushId, route) in routes {
            try? keychain.deleteKey(for: Self.routeTag(pushId))
            try keychain.saveKey(JSONEncoder().encode(route), with: Self.routeTag(pushId))
        }
        try? keychain.deleteKey(for: indexTag)
        try keychain.saveKey(JSONEncoder().encode(routes.keys.sorted()), with: indexTag)
    }

    private static func chatIndexTag(productId: String) -> String {
        "truapi.push.chat.\(productId)"
    }

    private static func routeTag(_ pushId: String) -> String {
        "truapi.push.route.\(pushId)"
    }
}
