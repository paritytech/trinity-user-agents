import Foundation
import SDKLogger
import SubstrateSdk
import AssetExchange
import XcmTransfer
import ChainRegistry

enum TokenDepositEventMatcherFactoryError: Error {
    case unexpectedChainAsset(ChainAssetProtocol)
}

final class TokenDepositEventMatcherFactory {
    let logger: LoggerProtocol

    init(logger: LoggerProtocol) {
        self.logger = logger
    }
}

extension TokenDepositEventMatcherFactory: TokenDepositEventMatcherFactoryProtocol {
    func createMatcher(
        for asset: ChainAssetProtocol
    ) -> [TokenDepositEventMatching]? {
        guard let chainAssetModel = asset as? ChainAsset else {
            return nil
        }

        return try? CustomAssetMapper(
            type: chainAssetModel.asset.type,
            typeExtras: chainAssetModel.asset.typeExtras
        ).mapAssetWithExtras(
            .init(
                nativeHandler: { [logger] in
                    [
                        NativeTokenMintedEventMatcher(logger: logger),
                        NativeTokenDepositedEventMatcher(logger: logger)
                    ]
                },
                statemineHandler: { [logger] extras in
                    [
                        PalletAssetsTokenDepositEventMatcher(extras: extras, logger: logger)
                    ]
                },
                ormlHandler: { [logger] extras in
                    [
                        TokensPalletDepositEventMatcher(extras: extras, logger: logger)
                    ]
                },
                ormlHydrationEvmHandler: { [logger] extras in
                    [
                        TokensPalletDepositEventMatcher(
                            extras: extras,
                            eventPath: CurrenciesPallet.depositedEventPath,
                            logger: logger
                        )
                    ]
                }
            )
        )
    }
}
