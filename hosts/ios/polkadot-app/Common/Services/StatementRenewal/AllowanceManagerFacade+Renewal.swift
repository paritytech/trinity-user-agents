import Foundation

extension AllowanceManagerFacade: AllowanceSlotsRenewing {
    func renewAllowanceSlots() async {
        await cleanupService.cleanup()

        await withDiscardingTaskGroup { group in
            for manager in allManagers {
                group.addTask {
                    do {
                        try await manager.renew()
                    } catch {
                        Logger.shared.error("Allowance slot renewal failed: \(error)")
                    }
                }
            }
        }
    }
}
