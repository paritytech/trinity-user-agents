import UIKit
import Foundation
import SDKLogger

@MainActor
protocol CallBackgroundTaskManaging {
    func beginBackgroundTask()
    func endBackgroundTask()
}

@MainActor
final class CallBackgroundTaskManager {
    static let callTaskName = "CallBackgroundTask"

    private var backgroundTaskIdentifier: UIBackgroundTaskIdentifier = .invalid
    private let application: UIApplication
    private let logger: LoggerProtocol

    init(
        application: UIApplication,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.application = application
        self.logger = logger
    }
}

private extension CallBackgroundTaskManager {
    func performEndBackgroundTask() {
        guard backgroundTaskIdentifier != .invalid else {
            return
        }

        let taskId = backgroundTaskIdentifier
        backgroundTaskIdentifier = .invalid

        application.endBackgroundTask(taskId)
        logger.debug("Ended background task: \(taskId.rawValue)")
    }
}

extension CallBackgroundTaskManager: CallBackgroundTaskManaging {
    func beginBackgroundTask() {
        guard backgroundTaskIdentifier == .invalid else {
            logger.debug("Background task already active")
            return
        }

        backgroundTaskIdentifier = application.beginBackgroundTask(
            withName: Self.callTaskName
        ) { [weak self] in
            self?.logger.debug("Expiration handler")
            self?.endBackgroundTask()
        }

        logger.debug("Started background task: \(backgroundTaskIdentifier.rawValue)")
    }

    func endBackgroundTask() {
        performEndBackgroundTask()
    }
}
