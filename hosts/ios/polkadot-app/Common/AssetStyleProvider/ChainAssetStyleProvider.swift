import UIKit
import ChainRegistry

struct ChainAssetStyle {
    let displayTitle: String
    let logo: UIImage?
    let brandColor: UIColor
    let mainTextColor: UIColor
    let secondaryTextColor: UIColor
}

protocol ChainAssetStyleProviding {
    func provide(for chainAsset: ChainAsset) -> ChainAssetStyle
}

extension ChainAssetStyleProviding {
    func provide(for chainAsset: ChainAsset) -> ChainAssetStyle {
        provide(for: chainAsset)
    }
}

final class ChainAssetStyleProvider {}

extension ChainAssetStyleProvider: ChainAssetStyleProviding {
    func provide(for chainAsset: ChainAsset) -> ChainAssetStyle {
        switch chainAsset.chainAssetId {
        #if UNSTABLE
            case SupportedAssets.dDollar:
                .init(
                    displayTitle: PaymentAssetBranding.shared.current.symbol,
                    logo: nil,
                    brandColor: .white,
                    mainTextColor: .black100,
                    secondaryTextColor: .black100
                )
        #else
            case SupportedAssets.pusdPPL:
                .init(
                    displayTitle: PaymentAssetBranding.shared.current.symbol,
                    logo: nil,
                    brandColor: .white,
                    mainTextColor: .black100,
                    secondaryTextColor: .black100
                )
        #endif
        case SupportedAssets.usdt:
            .init(
                displayTitle: "USDT",
                logo: .iconUsdt,
                brandColor: .assetUSDT,
                mainTextColor: .black100,
                secondaryTextColor: .black100
            )
        case SupportedAssets.usdc:
            .init(
                displayTitle: "USDC",
                logo: .iconUsdc,
                brandColor: .assetUSDС,
                mainTextColor: .black100,
                secondaryTextColor: .black100
            )
        case SupportedAssets.pas:
            .init(
                displayTitle: "PAS",
                logo: .iconDot,
                brandColor: .assetDOT,
                mainTextColor: .black100,
                secondaryTextColor: .black100
            )
        default:
            .init(
                displayTitle: chainAsset.asset.symbol,
                logo: nil,
                brandColor: .brandGreen,
                mainTextColor: .black100,
                secondaryTextColor: .fgPrimaryInverted
            )
        }
    }
}
