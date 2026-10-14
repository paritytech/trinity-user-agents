import Foundation

protocol PushRouteBuilding: AnyObject {
    func route(from userInfo: [AnyHashable: Any]) -> PushNavigationRoute?
}

final class PushRouteBuilder: PushRouteBuilding {
    private let logger: LoggerProtocol

    init(logger: LoggerProtocol = Logger.shared) {
        self.logger = logger
    }

    func route(from userInfo: [AnyHashable: Any]) -> PushNavigationRoute? {
        if userInfo[PushNotificationKeys.pushId] != nil {
            .contactChat(userInfo: userInfo)
        } else if let chatExtensionId = userInfo[PushNotificationKeys.chatExtensionId] as? ChatExtension.Id {
            .chatExtension(extensionId: chatExtensionId, userInfo: userInfo)
        } else if let url = productDeeplinkURL(from: userInfo) {
            .deeplink(url: url)
        } else {
            nil
        }
    }
}

private extension PushRouteBuilder {
    func productDeeplinkURL(from userInfo: [AnyHashable: Any]) -> URL? {
        let source = userInfo[PushNotificationKeys.pushSource] as? Int
        guard source == PushNotificationSource.products.rawValue,
              let deeplink = userInfo[PushNotificationKeys.deeplink] as? String,
              !deeplink.isEmpty else {
            return nil
        }

        if let url = URL(string: deeplink), url.scheme != nil {
            return url
        }
        let scheme = AppConfig.ProductUniversalLink.scheme
        return URL(string: "\(scheme)://\(deeplink)")
    }
}
