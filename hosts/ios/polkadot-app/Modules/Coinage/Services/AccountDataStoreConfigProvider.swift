import Foundation
import Revive

/// The `AccountDataStore` contract address from remote config (`account_data_store_config`): the
/// installation registry the core registers this installation in.
protocol AccountDataStoreConfigProviding: Sendable {
    /// `nil` until the remote config carries one.
    var contractAddress: EvmAddress? { get }
}

final class AccountDataStoreConfigProvider: AccountDataStoreConfigProviding, @unchecked Sendable {
    private let remoteConfig: @Sendable () -> RemoteAppConfig?

    init(remoteConfig: @escaping @Sendable () -> RemoteAppConfig? = { AppConfigProvider.shared.getRemoteConfig() }) {
        self.remoteConfig = remoteConfig
    }

    var contractAddress: EvmAddress? {
        remoteConfig()?.accountDataStoreContract
    }
}
