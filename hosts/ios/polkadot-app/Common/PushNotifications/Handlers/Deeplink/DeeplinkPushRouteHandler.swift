import Foundation

final class DeeplinkPushRouteHandler {
    private let deepLinkHandler: DeferredLinkHandling

    init(deepLinkHandler: DeferredLinkHandling = DeferredLinkHandler.shared) {
        self.deepLinkHandler = deepLinkHandler
    }
}

extension DeeplinkPushRouteHandler: PushRouteHandling {
    func handle(route: PushNavigationRoute) -> Bool {
        guard case let .deeplink(url) = route else {
            return false
        }

        deepLinkHandler.handle(with: url)

        return true
    }
}
