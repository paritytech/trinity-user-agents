import BigInt
import Foundation
import Foundation_iOS
import SubstrateSdk
import FoundationExt

protocol TransferAmountViewModelFactoryProtocol {
    var symbol: String { get }
    func amount(from value: Balance) -> String
}

final class PlainTransferAmountViewModelFactory {
    let targetAssetInfo: AssetBalanceDisplayInfo
    let integralFormatter: LocalizableDecimalFormatting
    let fractionFormatter: LocalizableDecimalFormatting

    init(
        targetAssetInfo: AssetBalanceDisplayInfo,
        formatterFactory: AssetBalanceFormatterFactoryProtocol
    ) {
        self.targetAssetInfo = targetAssetInfo

        integralFormatter = formatterFactory
            .createAssetPriceFormatter(
                for: targetAssetInfo.withoutSymbol,
                minimumFractionDigits: 0
            )
            .value(for: .current)

        fractionFormatter = formatterFactory
            .createAssetPriceFormatter(
                for: targetAssetInfo.withoutSymbol,
                minimumFractionDigits: targetAssetInfo.displayPrecision
            )
            .value(for: .current)
    }
}

extension PlainTransferAmountViewModelFactory: TransferAmountViewModelFactoryProtocol {
    var symbol: String {
        targetAssetInfo.symbol
    }

    func amount(from value: Balance) -> String {
        let amountDecimal = value.decimal(assetInfo: targetAssetInfo)
        let formatter: LocalizableDecimalFormatting =
            if amountDecimal.hasFraction {
                fractionFormatter
            } else {
                integralFormatter
            }

        return formatter.stringFromDecimal(amountDecimal) ?? ""
    }
}

final class TransferAmountViewModelFactory {
    let targetAssetInfo: AssetBalanceDisplayInfo
    let integralFormatter: LocalizableDecimalFormatting
    let fractionFormatter: LocalizableDecimalFormatting

    init(
        targetAssetInfo: AssetBalanceDisplayInfo,
        formatterFactory: AssetBalanceFormatterFactoryProtocol
    ) {
        self.targetAssetInfo = targetAssetInfo

        integralFormatter = formatterFactory
            .createAssetPriceFormatter(
                for: targetAssetInfo,
                minimumFractionDigits: 0
            )
            .value(for: .current)

        fractionFormatter = formatterFactory
            .createAssetPriceFormatter(
                for: targetAssetInfo,
                minimumFractionDigits: targetAssetInfo.displayPrecision
            )
            .value(for: .current)
    }
}

extension TransferAmountViewModelFactory: TransferAmountViewModelFactoryProtocol {
    var symbol: String {
        targetAssetInfo.symbol
    }

    func amount(from value: Balance) -> String {
        let amountDecimal = value.decimal(assetInfo: targetAssetInfo)
        let formatter: LocalizableDecimalFormatting =
            if amountDecimal.hasFraction {
                fractionFormatter
            } else {
                integralFormatter
            }

        return formatter.stringFromDecimal(amountDecimal) ?? ""
    }
}
