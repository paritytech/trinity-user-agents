import Foundation
import ChainStore
import SubstrateSdk

protocol ExtrinsicExtensionVersionProviding {
    /// Resolves the concrete `Extrinsic.Version` for the requested format. V4 carries no extension
    /// version; for V5 the extension version is sourced per-chain from remote config (default 0).
    func getExtensionVersion(for txVersion: Extrinsic.FormatVersion, chainId: ChainId) -> Extrinsic.Version
}

final class ExtrinsicExtensionVersionProvider {
    private let remoteConfig: RemoteConfigManaging

    init(remoteConfig: RemoteConfigManaging = FirebaseApplicationService.shared) {
        self.remoteConfig = remoteConfig
    }
}

extension ExtrinsicExtensionVersionProvider: ExtrinsicExtensionVersionProviding {
    func getExtensionVersion(for txVersion: Extrinsic.FormatVersion, chainId: ChainId) -> Extrinsic.Version {
        switch txVersion {
        case .V4:
            .V4
        case .V5:
            .V5(extensionVersion: remoteConfig.syncedTxExtensionVersions()[chainId] ?? 0)
        }
    }
}
