import Foundation
import BackgroundExecution
import CommonService

final class StatementRenewalLoop {
    static let interval: Duration = .seconds(60 * 60)

    private let durabilityRenewal: StatementDurabilityRenewing
    private let backgroundExecutor: BackgroundExecuting
    private let clock: any Clock<Duration>

    private var loopTask: Task<Void, Never>?

    /// The default ContinuousClock keeps counting while the app is suspended, so an overdue renewal runs on resume.
    init(
        durabilityRenewal: StatementDurabilityRenewing,
        backgroundExecutor: BackgroundExecuting,
        clock: any Clock<Duration> = ContinuousClock()
    ) {
        self.durabilityRenewal = durabilityRenewal
        self.backgroundExecutor = backgroundExecutor
        self.clock = clock
    }

    deinit {
        loopTask?.cancel()
    }
}

extension StatementRenewalLoop: ApplicationServiceProtocol {
    func setup() {
        loopTask?.cancel()
        loopTask = Task { [durabilityRenewal, backgroundExecutor, clock] in
            while !Task.isCancelled {
                _ = try? await backgroundExecutor.execute {
                    await durabilityRenewal.renewAll()
                }
                try? await clock.sleep(for: Self.interval)
            }
        }
    }

    func throttle() {
        loopTask?.cancel()
        loopTask = nil
    }
}
