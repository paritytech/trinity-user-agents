import Foundation
import Operation_iOS
import Products

protocol SSORemoteDisconnectApplying: Sendable {
    func applyDisconnect(from host: PolkadotSignInHost) async
}

final class SSORemoteDisconnectApplier: @unchecked Sendable, SSORemoteDisconnectApplying {
    private let hostRepository: AnyDataProviderRepository<PolkadotSignInHost>
    private let localDeviceRepository: AnyDataProviderRepository<Chat.LocalDevice>
    private let logger: LoggerProtocol

    init(
        hostRepositoryFactory: PolkadotSignInHostRepositoryMaking = PolkadotSignInHostRepositoryFactory(),
        localDeviceRepositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory(),
        logger: LoggerProtocol = Logger.shared
    ) {
        hostRepository = hostRepositoryFactory.createRepository(forFilter: nil)
        localDeviceRepository = localDeviceRepositoryFactory.createRepository(forFilter: nil)
        self.logger = logger
    }

    func applyDisconnect(from host: PolkadotSignInHost) async {
        let statementAccountId = host.accountId
        let deviceIdentifier = statementAccountId.toHex()

        do {
            let removeHostOperation = hostRepository.saveOperation({ [] }, { [host.identifier] })
            try await removeHostOperation.asyncExecute()
            logger.debug("Removed host \(host.name)")
        } catch {
            logger.error("Failed to remove host \(host.name): \(error)")
        }

        do {
            let removeDeviceOperation = localDeviceRepository.saveOperation({ [] }, { [deviceIdentifier] })
            try await removeDeviceOperation.asyncExecute()
            logger.debug("Removed local device for host \(host.name)")
        } catch {
            logger.error("Failed to remove local device for host \(host.name): \(error)")
        }
    }
}
