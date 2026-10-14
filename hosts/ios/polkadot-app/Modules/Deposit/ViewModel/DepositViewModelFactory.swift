import Foundation
import SubstrateSdk
import Foundation_iOS
import UIKit
import ChainRegistry

protocol DepositViewModelMaking {
    func makeAssetsViewModel(for depositAsset: ChainAsset) -> DepositAssetsViewModel

    func makeSummaryViewModel(
        for summary: DepositSummary,
        assetIn: ChainAsset,
        assetOut: ChainAsset
    ) -> DepositSummaryViewModel

    func makeOperationsViewModel(
        for operations: [DepositOperationModel],
        assetOut: ChainAsset
    ) -> [DepositOperationViewModel]
}

final class DepositViewModelFactory {
    let formatterFactory: AssetBalanceFormatterFactoryProtocol
    let styleProvider: ChainAssetStyleProviding
    let selectedCurrencyManager: SelectedCurrencyManaging

    private var assetFormatters: [ChainAssetId: LocalizableDecimalFormatting] = [:]
    private var priceFormatter: LocalizableDecimalFormatting?

    init(
        formatterFactory: AssetBalanceFormatterFactoryProtocol = AssetBalanceFormatterFactory(),
        styleProvider: ChainAssetStyleProviding = ChainAssetStyleProvider(),
        selectedCurrencyManager: SelectedCurrencyManaging = SelectedCurrencyManager.shared
    ) {
        self.formatterFactory = formatterFactory
        self.styleProvider = styleProvider
        self.selectedCurrencyManager = selectedCurrencyManager
    }
}

private extension DepositViewModelFactory {
    func getAssetFormatter(for chainAsset: ChainAsset) -> LocalizableDecimalFormatting {
        if let formatter = assetFormatters[chainAsset.chainAssetId] {
            return formatter
        }

        let formatter = formatterFactory.createTokenFormatter(for: chainAsset.assetDisplayInfo).value(for: .current)

        assetFormatters[chainAsset.chainAssetId] = formatter

        return formatter
    }

    func getPriceFormatter() -> LocalizableDecimalFormatting {
        if let priceFormatter {
            return priceFormatter
        }

        let displayInfo = AssetBalanceDisplayInfo.from(currency: selectedCurrencyManager.selectedCurrency)
        let formatter = formatterFactory.createAssetPriceFormatter(for: displayInfo).value(for: .current)

        priceFormatter = formatter

        return formatter
    }
}

extension DepositViewModelFactory: DepositViewModelMaking {
    func makeAssetsViewModel(for depositAsset: ChainAsset) -> DepositAssetsViewModel {
        let style = styleProvider.provide(for: depositAsset)
        let icon = style.logo.map { StaticImageViewModel(image: $0) }

        return DepositAssetsViewModel(
            assetName: style.displayTitle,
            assetColor: style.brandColor,
            assetIcon: icon,
            network: depositAsset.chain.name
        )
    }

    func makeSummaryViewModel(
        for summary: DepositSummary,
        assetIn: ChainAsset,
        assetOut: ChainAsset
    ) -> DepositSummaryViewModel {
        let style = styleProvider.provide(for: assetIn)
        let icon = style.logo.map { StaticImageViewModel(image: $0) }

        let assetInFormatter = getAssetFormatter(for: assetIn)

        let assetOutFormatter = getAssetFormatter(for: assetOut)

        let priceFormatter = getPriceFormatter()

        let minimumAmountString = assetInFormatter.stringFromDecimal(
            summary.minimumAmount.decimal(assetInfo: assetIn.assetDisplayInfo)
        )

        let rateAmountInString = assetInFormatter.stringFromDecimal(1.0)
        let rateAmountOutString = assetOutFormatter.stringFromDecimal(summary.rate)
        let feeString = priceFormatter.stringFromDecimal(summary.feeInUsd)

        return DepositSummaryViewModel(
            asset: style.displayTitle,
            assetIcon: icon,
            network: assetIn.chain.name,
            minimumAmount: minimumAmountString,
            address: summary.depositAddress,
            rateAmountIn: rateAmountInString ?? "",
            rateAmountOut: rateAmountOutString ?? "",
            fee: feeString ?? "",
            qrCodeImage: summary.qrCode
        )
    }

    func makeOperationsViewModel(
        for operations: [DepositOperationModel],
        assetOut: ChainAsset
    ) -> [DepositOperationViewModel] {
        operations.reversed().map { operation in
            let assetInFormatter = getAssetFormatter(for: operation.assetIn)
            let assetOutFormatter = getAssetFormatter(for: assetOut)

            let amountInString = assetInFormatter.stringFromDecimal(
                operation.execution.execLabel.balance.decimal(
                    assetInfo: operation.assetIn.assetDisplayInfo
                )
            )

            let amountOutString = assetOutFormatter.stringFromDecimal(
                operation.execution.amountOut.decimal(
                    assetInfo: assetOut.assetDisplayInfo
                )
            )

            return DepositOperationViewModel(
                id: UUID().uuidString,
                amountIn: amountInString ?? "",
                amountOut: amountOutString ?? "",
                status: operation.execution.status
            )
        }
    }
}
