import Foundation
import Keystore_iOS

/// The room of a product that a push id names, with the key of its notifications. The app makes it for
/// `push.address`; the notification service extension reads it to open a product push.
struct ProductPushRoute: Codable, Equatable {
    let productId: String
    let roomId: String
    let key: Data
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

    /// The push id of a room, when the app made one.
    func pushId(productId: String, roomId: String) -> String? {
        guard let data = try? keychain.fetchKey(for: Self.roomTag(productId: productId, roomId: roomId)) else {
            return nil
        }
        return String(data: data, encoding: .utf8)
    }

    func save(_ route: ProductPushRoute, pushId: String) throws {
        let routeTag = Self.routeTag(pushId)
        let roomTag = Self.roomTag(productId: route.productId, roomId: route.roomId)
        try? keychain.deleteKey(for: routeTag)
        try? keychain.deleteKey(for: roomTag)
        try keychain.saveKey(JSONEncoder().encode(route), with: routeTag)
        try keychain.saveKey(Data(pushId.utf8), with: roomTag)
    }

    private static func routeTag(_ pushId: String) -> String {
        "truapi.push.route.\(pushId)"
    }

    private static func roomTag(productId: String, roomId: String) -> String {
        "truapi.push.room.\(productId)/\(roomId)"
    }
}

/// The notification in a product push: the JSON object `{v, title, body}` that the core of the sending device
/// encrypted with XChaCha20-Poly1305 under the key of the route, with the associated data `truapi/push/v1`. The push
/// carries the hex of the 24-byte nonce and the ciphertext.
struct ProductPushNotification: Decodable, Equatable {
    let title: String
    let body: String

    static let associatedData = Data("truapi/push/v1".utf8)

    /// The notification of `messageHex`, or nil when it does not open with `key`.
    static func open(messageHex: String, key: Data) -> ProductPushNotification? {
        let hex = messageHex.hasPrefix("0x") ? String(messageHex.dropFirst(2)) : messageHex
        guard let bytes = Data(productPushHex: hex), bytes.count > 24 + 16 else { return nil }
        let opened = XChaCha20Poly1305.open(
            bytes.dropFirst(24),
            key: key,
            nonce: bytes.prefix(24),
            authenticating: associatedData
        )
        return opened.flatMap { try? JSONDecoder().decode(ProductPushNotification.self, from: $0) }
    }
}

private extension Data {
    init?(productPushHex hex: String) {
        guard hex.count.isMultiple(of: 2) else { return nil }
        var bytes = Data(capacity: hex.count / 2)
        var index = hex.startIndex
        while index < hex.endIndex {
            let next = hex.index(index, offsetBy: 2)
            guard let byte = UInt8(hex[index ..< next], radix: 16) else { return nil }
            bytes.append(byte)
            index = next
        }
        self = bytes
    }
}
