import Foundation
import Operation_iOS
import Products

protocol SSORemoteDisconnectApplying: Sendable {
    func applyDisconnect(from host: PolkadotSignInHost) async
}

final class SSORemoteDisconnectApplier: @unchecked Sendable, SSORemoteDisconnectApplying {
    private let hostRepository: AnyDataProviderRepository<PolkadotSignInHost>
    private let localDeviceRepository: AnyDataProviderRepository<Chat.LocalDevice>
    private let injectedBroadcaster: DeviceMessageBroadcasting?
    private let tldProvider: DotNsTldProviding
    private let logger: LoggerProtocol

    init(
        hostRepositoryFactory: PolkadotSignInHostRepositoryMaking = PolkadotSignInHostRepositoryFactory(),
        localDeviceRepositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory(),
        deviceMessageBroadcaster: DeviceMessageBroadcasting? = nil,
        tldProvider: DotNsTldProviding = DotNsTldProviderFacade.shared,
        logger: LoggerProtocol = Logger.shared
    ) {
        hostRepository = hostRepositoryFactory.createRepository(forFilter: nil)
        localDeviceRepository = localDeviceRepositoryFactory.createRepository(forFilter: nil)
        injectedBroadcaster = deviceMessageBroadcaster
        self.tldProvider = tldProvider
        self.logger = logger
    }

    private func makeBroadcaster() throws -> DeviceMessageBroadcasting {
        if let injectedBroadcaster {
            return injectedBroadcaster
        }

        let tld = try tldProvider.currentTldOrError()
        return MultideviceComponentFactory.makeDeviceMessageBroadcaster(
            messageExchangeModeProvider: ChatMessageExchangeModeProvider(tld: tld)
        )
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

        do {
            try await makeBroadcaster().broadcastDeviceRemoved(
                statementAccountId: statementAccountId
            )
            logger.debug("Broadcast deviceRemoved for host \(host.name)")
        } catch {
            logger.error("Failed to broadcast deviceRemoved for host \(host.name): \(error)")
        }
    }
}
