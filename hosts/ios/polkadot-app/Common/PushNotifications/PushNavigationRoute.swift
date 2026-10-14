import Foundation

enum PushNavigationRoute {
    case contactChat(userInfo: [AnyHashable: Any])
    case chatExtension(extensionId: String, userInfo: [AnyHashable: Any])
    case deeplink(url: URL)
}
