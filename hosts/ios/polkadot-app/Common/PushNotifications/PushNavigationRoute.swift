import Foundation

enum PushNavigationRoute {
    case chatExtension(extensionId: String, userInfo: [AnyHashable: Any])
    /// A room of the chat product, which draws as a person chat (HX2).
    case productRoom(productId: String, roomId: String)
    case deeplink(url: URL)
}
