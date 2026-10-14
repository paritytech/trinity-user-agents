import Foundation
import CommonService
import FoundationExt
import Individuality

final class AllowanceRenewalService {
    private let managerFacade: AllowanceManagerFacade
    private let appStateStreamFactory: ApplicationStateStreamFactory
    private let logger: LoggerProtocol

    private var observerTask: Task<Void, Never>?

    init(
        managerFacade: AllowanceManagerFacade,
        appStateStreamFactory: ApplicationStateStreamFactory,
        logger: LoggerProtocol
    ) {
        self.managerFacade = managerFacade
        self.appStateStreamFactory = appStateStreamFactory
        self.logger = logger
    }

    deinit {
        observerTask?.cancel()
    }
}

extension AllowanceRenewalService: ApplicationServiceProtocol {
    func setup() {
        observerTask?.cancel()
        observerTask = Task { [weak self] in
            await self?.renewAllowanceSlots()

            let foregroundStream = self?.appStateStreamFactory.stream(for: .willEnterForeground)
            guard let foregroundStream else { return }
            for await _ in foregroundStream {
                guard !Task.isCancelled else { return }
                await self?.renewAllowanceSlots()
            }
        }
    }

    func throttle() {
        observerTask?.cancel()
        observerTask = nil
    }
}

private extension AllowanceRenewalService {
    func renewAllowanceSlots() async {
        await withDiscardingTaskGroup { group in
            for manager in managerFacade.allManagers {
                group.addTask { [logger] in
                    do {
                        try await manager.renew()
                    } catch {
                        logger.error("Allowance slot renewal failed: \(error)")
                    }
                }
            }
        }
    }
}
