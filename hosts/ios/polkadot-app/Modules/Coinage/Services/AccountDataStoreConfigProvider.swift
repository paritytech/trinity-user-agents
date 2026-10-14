import Coinage
import Foundation
import Revive
import SubstrateSdk

/// The `AccountDataStore` contract address from remote config (`account_data_store_config`), or nil
/// until the payload carries one.
final class AccountDataStoreConfigProvider: AccountDataStoreConfigProviding, @unchecked Sendable {
    private let remoteConfig: @Sendable () -> RemoteAppConfig?

    init(remoteConfig: @escaping @Sendable () -> RemoteAppConfig? = { AppConfigProvider.shared.getRemoteConfig() }) {
        self.remoteConfig = remoteConfig
    }

    func contractAddress() async -> EvmAddress? {
        remoteConfig()?.accountDataStoreContract
    }
}
