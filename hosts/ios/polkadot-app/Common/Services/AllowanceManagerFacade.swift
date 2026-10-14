import Foundation
import Individuality
import ChainRegistry

struct AllowanceManagerFacade {
    let bulletInManager: AllowanceManaging
    let sssManager: AllowanceManaging
    let smartContractManager: AllowanceManaging

    var allManagers: [AllowanceManaging] {
        [bulletInManager, sssManager, smartContractManager]
    }
}

extension AllowanceManagerFacade {
    static func create(
        chainRegistry: ChainRegistryProtocol = ChainRegistryFacade.sharedRegistry
    ) -> AllowanceManagerFacade? {
        guard
            let bulletInManager = BulletInAllowanceManager.create(chainRegistry: chainRegistry),
            let sssManager = SSStoreAllowanceManager.create(chainRegistry: chainRegistry),
            let smartContractManager = PGASAllowanceManager.create(chainRegistry: chainRegistry)
        else {
            return nil
        }

        return AllowanceManagerFacade(
            bulletInManager: bulletInManager,
            sssManager: sssManager,
            smartContractManager: smartContractManager
        )
    }
}
