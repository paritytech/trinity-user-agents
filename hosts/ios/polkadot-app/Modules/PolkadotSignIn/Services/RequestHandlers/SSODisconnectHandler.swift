import Foundation

final class SSODisconnectHandler: SSORequestHandling {
    private let applier: SSORemoteDisconnectApplying
    private let logger: LoggerProtocol

    init(
        applier: SSORemoteDisconnectApplying = SSORemoteDisconnectApplier(),
        logger: LoggerProtocol = Logger.shared
    ) {
        self.applier = applier
        self.logger = logger
    }

    func canHandle(_ message: PolkadotHostRemoteMessage) -> Bool {
        guard case .disconnected = message.latestContent() else { return false }
        return true
    }

    func handle(
        message _: PolkadotHostRemoteMessage,
        from host: PolkadotSignInHost
    ) async {
        logger.info("Got disconnected message from host \(host.name)")
        await applier.applyDisconnect(from: host)
    }
}
