import Foundation
import SubstrateSdk

enum SupportedAssets {
    #if UNSTABLE
        static let dDollar = ChainAssetId(chainId: KnownChainId.previewNetPeople, assetId: 65)
        static let usdt = ChainAssetId(chainId: KnownChainId.previewAH, assetId: 1)
        static let usdc = ChainAssetId(chainId: KnownChainId.previewAH, assetId: 2)
        static let pas = ChainAssetId(chainId: KnownChainId.previewAH, assetId: 0)
        static let pgasAH = ChainAssetId(chainId: KnownChainId.previewAH, assetId: 4)

        static let dotUnstablePPL = ChainAssetId(chainId: KnownChainId.previewNetPeople, assetId: 0)
    #elseif NIGHTLY || SAFETYNET
        static let pusdPPL = ChainAssetId(chainId: KnownChainId.paseoPeople, assetId: 1)
        static let pasPPL = ChainAssetId(chainId: KnownChainId.paseoPeople, assetId: 0)

        static let usdt = ChainAssetId(chainId: KnownChainId.paseoAH, assetId: 1)
        static let usdc = ChainAssetId(chainId: KnownChainId.paseoAH, assetId: 2)
        static let pusdAH = ChainAssetId(chainId: KnownChainId.paseoAH, assetId: 3)
        static let pas = ChainAssetId(chainId: KnownChainId.paseoAH, assetId: 0)
        static let pgasAH = ChainAssetId(chainId: KnownChainId.paseoAH, assetId: 4)
    #else
        static let pusdPPL = ChainAssetId(chainId: KnownChainId.releasePeople, assetId: 1)
        static let pasPPL = ChainAssetId(chainId: KnownChainId.releasePeople, assetId: 0)

        static let usdt = ChainAssetId(chainId: KnownChainId.releaseAH, assetId: 1)
        static let usdc = ChainAssetId(chainId: KnownChainId.releaseAH, assetId: 2)
        static let pusdAH = ChainAssetId(chainId: KnownChainId.releaseAH, assetId: 3)
        static let pas = ChainAssetId(chainId: KnownChainId.releaseAH, assetId: 0)
        static let pgasAH = ChainAssetId(chainId: KnownChainId.releaseAH, assetId: 4)
    #endif
}
