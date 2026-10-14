import Foundation
import Operation_iOS
import SubstrateSdk
import Individuality

final class RemoveDeviceInteractor {
    weak var presenter: RemoveDeviceInteractorOutputProtocol?

    private let localDeviceRepository: AnyDataProviderRepository<Chat.LocalDevice>
    private let serviceCoordinator: ServiceCoordinatorProtocol
    private let deviceMessageBroadcaster: DeviceMessageBroadcasting
    private let logger: LoggerProtocol

    init(
        localDeviceRepository: AnyDataProviderRepository<Chat.LocalDevice>,
        serviceCoordinator: ServiceCoordinatorProtocol,
        deviceMessageBroadcaster: DeviceMessageBroadcasting,
        logger: LoggerProtocol = Logger.shared
    ) {
        self.localDeviceRepository = localDeviceRepository
        self.serviceCoordinator = serviceCoordinator
        self.deviceMessageBroadcaster = deviceMessageBroadcaster
        self.logger = logger
    }
}

extension RemoveDeviceInteractor: RemoveDeviceInteractorInputProtocol {
    func removeDevice(identifier: String) {
        Task { [weak self] in
            do {
                guard let self else { return }

                let statementAccountId = try Data(hexString: identifier)
                try await deviceMessageBroadcaster.broadcastDeviceRemoved(
                    statementAccountId: statementAccountId
                )

                logger.debug("Disconnecting host for device \(identifier)...")
                try await serviceCoordinator.signInHostCoordinator.disconnectHost(
                    byAccountId: statementAccountId
                )
                logger.debug("Disconnected host for device \(identifier)")

                await releaseAllowance(for: statementAccountId)

                let operation = localDeviceRepository.saveOperation({ [] }, { [identifier] })
                try await operation.asyncExecute()

                await reportSuccess()
            } catch {
                self?.logger.error("Failed to remove device: \(error)")
                await self?.reportFailure(error)
            }
        }
    }
}

private extension RemoveDeviceInteractor {
    func releaseAllowance(for accountId: AccountId) async {
        do {
            try await serviceCoordinator.allowanceManagerFacade.sssManager.release(accountId: accountId)
            logger.debug("Released allowance for device \(accountId.toHex())")
        } catch {
            logger.error("Failed to release allowance: \(error)")
        }
    }

    @MainActor
    func reportSuccess() {
        presenter?.didRemoveDevice()
    }

    @MainActor
    func reportFailure(_ error: Error) {
        presenter?.didFailToRemoveDevice(error: error)
    }
}
