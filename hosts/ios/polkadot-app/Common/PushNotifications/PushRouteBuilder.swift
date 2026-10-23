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
        if let room = userInfo.productRoom {
            .productRoom(productId: room.productId, roomId: room.roomId)
        } else if let chatExtensionId = userInfo[PushNotificationKeys.chatExtensionId] as? ChatExtension.Id {
            .chatExtension(extensionId: chatExtensionId, userInfo: userInfo)
        } else if let url = productDeeplinkURL(from: userInfo) {
            .deeplink(url: url)
        } else {
            nil
        }
    }
}

extension [AnyHashable: Any] {
    /// The room of the chat product that a notification opens.
    var productRoom: (productId: String, roomId: String)? {
        guard
            let productId = self[PushNotificationKeys.productId] as? String,
            let roomId = self[PushNotificationKeys.productRoomId] as? String
        else {
            return nil
        }
        return (productId, roomId)
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
