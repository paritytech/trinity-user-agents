import Foundation
import SubstrateSdk
import ChainRegistry

enum AppConfig {
    static var termsOfUseLink: URL { Brand.termsURL }

    static var privacyPolicyLink: URL { Brand.privacyURL }

    static var contactEmail: String { Brand.contactEmail }

    static let timestampRefreshInterval: TimeInterval = 60

    #if TESTNET_FEATURE
        // Debug-only DIM2 game dashboard. Gated by TESTNET_FEATURE
        // so Release builds never reach this endpoint.
        static var gameDashboardBaseURL: URL {
            AppConfigProvider.shared.getRemoteConfig()!.gameDashboardUrl!
        }
    #endif

    enum Assets {
        #if UNSTABLE
            static let mainAsset: ChainAssetId = SupportedAssets.dDollar
            static let fiatOnrampFundedAsset: ChainAssetId = SupportedAssets.usdt
        #else
            static let mainAsset: ChainAssetId = SupportedAssets.pusdPPL
            static let fiatOnrampFundedAsset: ChainAssetId = SupportedAssets.pusdAH
        #endif

        static let pgasAsset: ChainAssetId = SupportedAssets.pgasAH

        static var all: [ChainAssetId] {
            [mainAsset]
        }

        #if UNSTABLE
            static let fundingAssets: [ChainAssetId] = [
                SupportedAssets.pas,
                SupportedAssets.usdt,
                SupportedAssets.usdc
            ]
            static let fundedAsset = SupportedAssets.dDollar
            static let dimAsset = SupportedAssets.dotUnstablePPL
        #else
            static let fundingAssets: [ChainAssetId] = [
                SupportedAssets.pas,
                SupportedAssets.usdt,
                SupportedAssets.usdc
            ]
            static let fundedAsset = SupportedAssets.pusdPPL
            static let dimAsset = SupportedAssets.pasPPL
        #endif
    }

    enum Chains {
        static var chatChain: ChainModel.Id {
            #if UNSTABLE
                KnownChainId.previewNetPeople
            #elseif NIGHTLY || SAFETYNET
                KnownChainId.paseoPeople
            #else
                KnownChainId.releasePeople
            #endif
        }

        static var usernameChain: ChainModel.Id {
            #if UNSTABLE
                KnownChainId.previewNetPeople
            #elseif NIGHTLY || SAFETYNET
                KnownChainId.paseoPeople
            #else
                KnownChainId.releasePeople
            #endif
        }

        static var bulletInChain: ChainModel.Id {
            #if UNSTABLE
                KnownChainId.previewNetBulletIn
            #elseif NIGHTLY || SAFETYNET
                KnownChainId.paseoBulletIn
            #else
                KnownChainId.releaseBulletIn
            #endif
        }

        static var fundingChain: ChainModel.Id {
            #if UNSTABLE
                KnownChainId.previewAH
            #elseif NIGHTLY || SAFETYNET
                KnownChainId.paseoAH
            #else
                KnownChainId.releaseAH
            #endif
        }

        static var swappingChain: ChainModel.Id {
            #if UNSTABLE
                KnownChainId.previewAH
            #elseif NIGHTLY || SAFETYNET
                KnownChainId.paseoAH
            #else
                KnownChainId.releaseAH
            #endif
        }

        static var assethubChain: ChainModel.Id {
            #if UNSTABLE
                KnownChainId.previewAH
            #elseif NIGHTLY || SAFETYNET
                KnownChainId.paseoAH
            #else
                KnownChainId.releaseAH
            #endif
        }

        static var usdtChain: ChainModel.Id { assethubChain }
    }
}
