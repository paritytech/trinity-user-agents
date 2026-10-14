import Foundation

class ChatPushRouteHandler {
    private(set) var logger: LoggerProtocol
    private(set) weak var visibilityReporter: PushForegroundVisibilityReporting?
    let moduleNavigator: ModuleNavigating

    init(
        moduleNavigator: ModuleNavigating,
        logger: LoggerProtocol,
        visibilityReporter: PushForegroundVisibilityReporting?
    ) {
        self.moduleNavigator = moduleNavigator
        self.logger = logger
        self.visibilityReporter = visibilityReporter
    }

    func handle(_: PushNavigationRoute) -> Bool {
        fatalError("Must be overriden by subclass")
    }
}

extension ChatPushRouteHandler: PushRouteHandling {
    func handle(route: PushNavigationRoute) -> Bool {
        handle(route)
    }
}
