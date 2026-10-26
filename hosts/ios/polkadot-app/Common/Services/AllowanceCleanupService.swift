import Foundation
import Individuality

protocol AllowanceCleanupServicing: Sendable {
    /// Releases allowances their features no longer need, so renewal stops paying for them.
    func cleanup() async
}

final class AllowanceCleanupService: @unchecked Sendable {
    struct Target {
        let manager: AllowanceManaging
        let strategy: AllowanceReleaseStrategy
    }

    private let targets: [Target]
    private let logger: LoggerProtocol

    init(targets: [Target], logger: LoggerProtocol) {
        self.targets = targets
        self.logger = logger
    }
}

extension AllowanceCleanupService: AllowanceCleanupServicing {
    func cleanup() async {
        for target in targets {
            do {
                try await target.manager.release(with: target.strategy)
            } catch {
                logger.error("Allowance cleanup failed: \(error)")
            }
        }
    }
}
