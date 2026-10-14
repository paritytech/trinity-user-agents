import Foundation

final class ChatExtensionPushRouteHandler: ChatPushRouteHandler {
    private let routers: [ChatExtensionPushRouting]

    init(
        routers: [ChatExtensionPushRouting],
        moduleNavigator: ModuleNavigating,
        logger: LoggerProtocol = Logger.shared,
        visibilityReporter: PushForegroundVisibilityReporting?
    ) {
        self.routers = routers

        super.init(
            moduleNavigator: moduleNavigator,
            logger: logger,
            visibilityReporter: visibilityReporter
        )
    }

    override func handle(_ route: PushNavigationRoute) -> Bool {
        guard case let .chatExtension(extensionId, userInfo) = route else {
            return false
        }

        resolveExtension(for: extensionId, userInfo)

        return true
    }
}

private extension ChatExtensionPushRouteHandler {
    func resolveExtension(
        for extensionId: ChatExtension.Id,
        _ userInfo: [AnyHashable: Any]
    ) {
        let chatId = Chat.Id.chatExtension(extensionId)

        routers.forEach { router in
            router.process(userInfo: userInfo) { [weak self] in
                guard self?.visibilityReporter?.isChatVisible(for: chatId) != true else {
                    return
                }

                Task { @MainActor [weak self] in
                    self?.moduleNavigator.openChat(chatId)
                }
            }
        }
    }
}
