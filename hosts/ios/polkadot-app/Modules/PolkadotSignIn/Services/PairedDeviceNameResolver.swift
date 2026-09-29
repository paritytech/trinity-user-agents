import Foundation
import Operation_iOS

protocol PairedDeviceNameResolving: Sendable {
    /// The name Linked Devices shows for the paired device whose statement
    /// account is `statementAccountId`, or nil when the device is unknown.
    func deviceName(forStatementAccountId statementAccountId: Data) async -> String?
}

/// Opens the device store on lookup rather than on construction: presenters
/// and handlers that default to this resolver must be constructible where the
/// app-group store is unavailable, and only a relayed request asks for a name.
final class PairedDeviceNameResolver: PairedDeviceNameResolving, @unchecked Sendable {
    private let makeRepositoryFactory: () -> LocalDeviceRepositoryMaking

    init(makeRepositoryFactory: @escaping () -> LocalDeviceRepositoryMaking = { LocalDeviceRepositoryFactory() }) {
        self.makeRepositoryFactory = makeRepositoryFactory
    }

    func deviceName(forStatementAccountId statementAccountId: Data) async -> String? {
        try? await makeRepositoryFactory()
            .createRepository(forFilter: nil)
            .fetchOperation(by: { statementAccountId.toHex() }, options: RepositoryFetchOptions())
            .asyncExecute()?
            .displayDeviceName
    }
}
