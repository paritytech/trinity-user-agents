import Foundation
import Products

extension ProductPermissionGuard {
    /// Assembles the guard with all permission handlers wired to one
    /// requester/repository pair.
    static func create(
        router: ProductPermissionRouting,
        repository: ProductPermissionRepositoryProtocol = ProductPermissionRepository(),
        osAsker: OSPermissionAsking = OSPermissionAsker()
    ) -> ProductPermissionGuard {
        let requester = ProductPermissionRequesterFactory.create(router: router)

        let networkHandler = NetworkAccessPermissionHandler(
            repository: repository,
            requester: requester
        )
        let remoteHandler = RemotePermissionHandler(
            repository: repository,
            requester: requester
        )
        let deviceHandler = DeviceCapabilityPermissionHandler(
            repository: repository,
            requester: requester,
            osAsker: osAsker
        )
        let accountHandler = AccountAccessPermissionHandler(
            repository: repository,
            requester: requester
        )

        return ProductPermissionGuard(
            networkHandler: networkHandler,
            remoteHandler: remoteHandler,
            deviceHandler: deviceHandler,
            accountHandler: accountHandler,
            repository: repository,
            requester: requester
        )
    }
}
