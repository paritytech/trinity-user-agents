import Foundation
import UserNotifications

final class PushHandler {
    private let routeBuilder: PushRouteBuilding
    private let handlers: [PushRouteHandling]
    private let logger: LoggerProtocol

    init(
        routeBuilder: PushRouteBuilding,
        handlers: [PushRouteHandling],
        logger: LoggerProtocol = Logger.shared
    ) {
        self.routeBuilder = routeBuilder
        self.handlers = handlers
        self.logger = logger
    }
}

extension PushHandler: PushNotificationTapHandling {
    func handle(response: UNNotificationResponse, completion: @escaping () -> Void) {
        defer {
            completion()
        }

        let userInfo = response.notification.request.content.userInfo

        guard let route = routeBuilder.route(from: userInfo) else {
            logger.warning("Failed to build push route from: \(userInfo)")
            return
        }

        handle(route: route)
    }
}

private extension PushHandler {
    func handle(route: PushNavigationRoute) {
        for handler in handlers where handler.handle(route: route) {
            return
        }

        logger.warning("No handler registered for route \(route)")
    }
}
