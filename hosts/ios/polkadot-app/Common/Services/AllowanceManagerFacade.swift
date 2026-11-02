import Foundation
import Individuality
import ChainRegistry
import KeyDerivation
import Products

// Immutable references to managers that guard their own state.
struct AllowanceManagerFacade: @unchecked Sendable {
    let bulletInManager: AllowanceManaging
    let sssManager: AllowanceManaging & StatementAccountFunding
    let smartContractManager: AllowanceManaging
    let cleanupService: AllowanceCleanupServicing

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
            smartContractManager: smartContractManager,
            cleanupService: makeCleanupService(sssManager: sssManager)
        )
    }

    private static func makeCleanupService(
        sssManager: AllowanceManaging,
        entropyManager: RootEntropyManaging = RootEntropyManager.shared,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared
    ) -> AllowanceCleanupServicing {
        let chatFilter = ChatAllowanceFilter(
            contactsStorageService: ContactsLocalStorageService(),
            signerManager: ChatSignerManager(
                entropyManager: entropyManager,
                tldProvider: tldProvider
            )
        )

        return AllowanceCleanupService(
            targets: [.init(manager: sssManager, strategy: .filter(chatFilter))],
            logger: Logger.shared
        )
    }
}
