import Foundation
import Products
import Revive
import SubstrateSdk

extension AppConfig {
    enum KnownIPFS {
        static var main: URL! {
            AppConfigProvider.shared.getRemoteConfig()!.ipfsGatewayUrl
        }
    }

    enum DotNs {
        private static var dotNsResolverAddress: String {
            AppConfigProvider.shared.getRemoteConfig()!.dotNsResolver!
        }

        /// Optional by design: an absent key disables manifest resolution and leaves legacy names
        /// working, so a value that will not decode has to degrade the same way rather than take
        /// every launch down with the rest of the config.
        private static var dotNsNameRegistryAddress: EvmAddress? {
            guard let raw = AppConfigProvider.shared.getRemoteConfig()?.dotNsNameRegistry else {
                return nil
            }

            return try? EvmAddressFormat.validate(raw.fromHex())
        }

        static let dotNsBrowse = "browse"

        static let dotNsGameWebview = "game-webview"

        static func config() throws -> DotNsConfig {
            let resolverAddress = try EvmAddressFormat.validate(Self.dotNsResolverAddress.fromHex())

            return DotNsConfig(
                contractsChainId: AppConfig.Chains.assethubChain,
                resolverContractAddress: resolverAddress,
                nameRegistryContractAddress: Self.dotNsNameRegistryAddress,
                ipfsGatewayBaseUrl: AppConfig.KnownIPFS.main
            )
        }
    }
}
