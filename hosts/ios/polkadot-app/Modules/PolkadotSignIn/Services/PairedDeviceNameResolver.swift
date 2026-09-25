import Foundation
import Operation_iOS

protocol PairedDeviceNameResolving: Sendable {
    /// The name Linked Devices shows for the paired device whose statement
    /// account is `statementAccountId`, or nil when the device is unknown.
    func deviceName(forStatementAccountId statementAccountId: Data) async -> String?
}

final class PairedDeviceNameResolver: PairedDeviceNameResolving, @unchecked Sendable {
    private let localDeviceRepository: AnyDataProviderRepository<Chat.LocalDevice>

    init(localDeviceRepositoryFactory: LocalDeviceRepositoryMaking = LocalDeviceRepositoryFactory()) {
        localDeviceRepository = localDeviceRepositoryFactory.createRepository(forFilter: nil)
    }

    func deviceName(forStatementAccountId statementAccountId: Data) async -> String? {
        try? await localDeviceRepository
            .fetchOperation(by: { statementAccountId.toHex() }, options: RepositoryFetchOptions())
            .asyncExecute()?
            .displayDeviceName
    }
}
