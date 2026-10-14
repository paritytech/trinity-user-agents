import Foundation
import Operation_iOS

protocol PolkadotHandshakePersisting {
    @discardableResult
    func saveDeviceData(
        from deviceData: HandshakeDeviceData,
        hostName: String
    ) async throws -> Chat.LocalDevice

    func saveHostData(
        deviceData: HandshakeDeviceData,
        metadata: HandshakeMetadata
    ) async throws
}

final class PolkadotHandshakePersistence {
    private let hostRepository: AnyDataProviderRepository<PolkadotSignInHost>
    private let localDeviceRepository: AnyDataProviderRepository<Chat.LocalDevice>

    init(
        hostRepositoryFactory: PolkadotSignInHostRepositoryMaking = PolkadotSignInHostRepositoryFactory(),
        localDeviceRepositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory()
    ) {
        hostRepository = hostRepositoryFactory.createRepository(forFilter: nil)
        localDeviceRepository = localDeviceRepositoryFactory.createRepository(forFilter: nil)
    }
}

extension PolkadotHandshakePersistence: PolkadotHandshakePersisting {
    @discardableResult
    func saveDeviceData(
        from deviceData: HandshakeDeviceData,
        hostName: String
    ) async throws -> Chat.LocalDevice {
        let device = Chat.LocalDevice(
            statementAccountId: deviceData.statementAccountId,
            encryptionPublicKey: deviceData.encryptionPublicKey,
            hostName: hostName,
            createdAt: Date(),
            hostVersion: deviceData.hostVersion,
            osType: deviceData.platformType,
            osVersion: deviceData.platformVersion
        )

        let operation = localDeviceRepository.saveOperation({ [device] }, { [] })
        try await operation.asyncExecute()

        return device
    }

    func saveHostData(
        deviceData: HandshakeDeviceData,
        metadata: HandshakeMetadata
    ) async throws {
        let entity = PolkadotSignInHost(
            accountId: deviceData.statementAccountId,
            publicKey: deviceData.encryptionPublicKey,
            name: metadata.name,
            iconUrl: metadata.iconUrl
        )
        let operation = hostRepository.saveOperation({ [entity] }, { [] })
        try await operation.asyncExecute()
    }
}
