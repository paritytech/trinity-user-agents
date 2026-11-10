import BackgroundExecution
import Foundation
import FoundationExt

enum StatementRenewalBackgroundTask {
    private static let identifierSuffix = ".statement-renewal"
    private static let logTag = "BGRenewal"

    private static let policy = BackgroundTaskRequestPolicy(
        earliestBeginDelay: 60 * 60,
        constraints: BackgroundTaskConstraints(requiresNetworkConnectivity: true, requiresExternalPower: false)
    )

    static let shared: ScheduledBackgroundTask? = make(baseLogger: Logger.shared)
}

private extension StatementRenewalBackgroundTask {
    static func make(baseLogger: LoggerProtocol) -> ScheduledBackgroundTask? {
        let logger = TaggedLogger(tag: logTag, logger: baseLogger)

        guard let bundleIdentifier = Bundle.main.bundleIdentifier else {
            logger.error("no bundle identifier, task not created")
            return nil
        }

        let work = StatementRenewalBackgroundWork()

        return ScheduledBackgroundTask(
            identifier: bundleIdentifier + identifierSuffix,
            policy: policy,
            dependencies: ScheduledBackgroundTaskDependencies(
                scheduler: BGTaskSchedulerAdapter(),
                dateProvider: NowDateProvider(),
                logger: logger
            ),
            work: { await work() }
        )
    }
}
