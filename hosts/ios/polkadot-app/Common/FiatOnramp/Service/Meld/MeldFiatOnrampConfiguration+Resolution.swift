import Foundation
import SubstrateSdk
import ChainRegistry
import SubstrateSdkExt

private enum MeldFiatOnrampChainCodeProvider {
    static func chainCode(for _: ChainModel) -> String {
        // NOTE: Use a proper chain mapping when MELD supports additional chains.
        "ASSETHUB"
    }
}

extension MeldFiatOnrampSupport {
    struct AssetContext {
        let chainAsset: ChainAsset
        let cryptoChain: String
        let cryptoCurrency: String
    }
}

extension MeldFiatOnrampConfiguration {
    static func resolveAssetContext(for chainAssetId: ChainAssetId) -> MeldFiatOnrampSupport.AssetContext? {
        guard let chain = ChainRegistryFacade.sharedRegistry.getChain(for: chainAssetId.chainId),
              let chainAsset = chain.chainAsset(for: chainAssetId.assetId) else {
            return nil
        }

        let cryptoChain = MeldFiatOnrampChainCodeProvider.chainCode(for: chain)
        // NOTE: Probably just a table mapping is better, this might not be good in all scenarios.
        let cryptoCurrency = "\(chainAsset.asset.symbol.uppercased())_\(cryptoChain)"

        return MeldFiatOnrampSupport.AssetContext(
            chainAsset: chainAsset,
            cryptoChain: cryptoChain,
            cryptoCurrency: cryptoCurrency
        )
    }

    static func resolveWalletAddress(
        for chainAsset: ChainAsset,
        walletRepo: WalletManagerRepositoryProtocol = .shared
    ) -> String? {
        guard let account = try? walletRepo.main().fetchAccount(for: chainAsset.chain) else {
            return nil
        }

        return try? account.accountId.toAddress(using: chainAsset.chain.chainFormat)
    }
}

extension MeldFiatOnrampSupport.ServiceProviderFilters {
    static func meldBase(
        countryCode: String,
        fiatCurrencyCode: String,
        cryptoChain: String,
        cryptoCurrency: String
    ) -> MeldFiatOnrampSupport.ServiceProviderFilters {
        MeldFiatOnrampSupport.ServiceProviderFilters(
            statuses: ["LIVE", "RECENTLY_ADDED"],
            categories: ["CRYPTO_ONRAMP"],
            accountFilter: false,
            countries: [countryCode],
            fiatCurrencies: [fiatCurrencyCode],
            cryptoChains: [cryptoChain],
            cryptoCurrencies: [cryptoCurrency]
        )
    }
}
