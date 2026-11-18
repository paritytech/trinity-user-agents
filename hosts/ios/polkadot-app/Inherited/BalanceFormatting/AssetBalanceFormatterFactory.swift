import Foundation
import Foundation_iOS

protocol AssetBalanceFormatterFactoryProtocol {
    func createInputFormatter(
        for info: AssetBalanceDisplayInfo
    ) -> LocalizableResource<NumberFormatter>

    func createTokenFormatter(
        for info: AssetBalanceDisplayInfo,
        roundingMode: NumberFormatter.RoundingMode
    ) -> LocalizableResource<LocalizableDecimalFormatting>

    func createAssetPriceFormatter(
        for info: AssetBalanceDisplayInfo,
        minimumFractionDigits: UInt16
    ) -> LocalizableResource<LocalizableDecimalFormatting>
}

extension AssetBalanceFormatterFactoryProtocol {
    func createAssetPriceFormatter(
        for info: AssetBalanceDisplayInfo
    ) -> LocalizableResource<LocalizableDecimalFormatting> {
        createAssetPriceFormatter(for: info, minimumFractionDigits: 0)
    }
}

extension AssetBalanceFormatterFactoryProtocol {
    func createTokenFormatter(
        for info: AssetBalanceDisplayInfo
    ) -> LocalizableResource<LocalizableDecimalFormatting> {
        createTokenFormatter(for: info, roundingMode: .down)
    }

    func createFeeTokenFormatter(
        for info: AssetBalanceDisplayInfo
    ) -> LocalizableResource<LocalizableDecimalFormatting> {
        createTokenFormatter(for: info, roundingMode: .up)
    }
}

class AssetBalanceFormatterFactory {
    private func createTokenFormatterCommon(
        for info: AssetBalanceDisplayInfo,
        minimumFractionDigits: UInt16,
        roundingMode: NumberFormatter.RoundingMode,
        preferredPrecisionOffset: UInt8 = 0,
        usesSuffixForBigNumbers: Bool = true
    ) -> LocalizableResource<LocalizableDecimalFormatting> {
        let formatter = createCompoundFormatter(
            for: info.displayPrecision,
            minimumFractionDigits: minimumFractionDigits,
            roundingMode: roundingMode,
            prefferedPrecisionOffset: preferredPrecisionOffset,
            usesSuffixForBigNumber: usesSuffixForBigNumbers
        )

        let tokenFormatter = TokenFormatter(
            decimalFormatter: formatter,
            tokenSymbol: info.symbol,
            separator: info.symbolValueSeparator,
            position: info.symbolPosition
        )

        return LocalizableResource { locale in
            tokenFormatter.locale = locale
            return tokenFormatter
        }
    }

    private func createCompoundFormatter(
        for preferredPrecision: UInt16,
        minimumFractionDigits: UInt16,
        roundingMode: NumberFormatter.RoundingMode = .down,
        prefferedPrecisionOffset: UInt8 = 0,
        usesSuffixForBigNumber: Bool = true,
        usesIntGrouping: Bool? = nil
    ) -> LocalizableDecimalFormatting {
        var abbreviations: [BigNumberAbbreviation] = [
            BigNumberAbbreviation(
                threshold: 0,
                divisor: 1.0,
                suffix: "",
                formatter: DynamicPrecisionFormatter(
                    preferredPrecision: UInt8(preferredPrecision),
                    minimumFractionDigits: UInt8(minimumFractionDigits),
                    preferredPrecisionOffset: prefferedPrecisionOffset,
                    roundingMode: roundingMode
                )
            ),
            BigNumberAbbreviation(
                threshold: 1,
                divisor: 1.0,
                suffix: "",
                formatter: nil
            ),
            BigNumberAbbreviation(
                threshold: 10,
                divisor: 1.0,
                suffix: "",
                formatter: nil
            )
        ]

        if usesSuffixForBigNumber {
            // We don't want to use default formatter for abbreviations
            let abbreviationFormatter = NumberFormatter.decimalFormatter(
                precision: Int(preferredPrecision),
                rounding: roundingMode,
                usesIntGrouping: usesIntGrouping ?? true
            )

            abbreviations.append(contentsOf: [
                BigNumberAbbreviation(
                    threshold: 1_000_000,
                    divisor: 1_000_000.0,
                    suffix: "M",
                    formatter: abbreviationFormatter
                ),
                BigNumberAbbreviation(
                    threshold: 1_000_000_000,
                    divisor: 1_000_000_000.0,
                    suffix: "B",
                    formatter: abbreviationFormatter
                ),
                BigNumberAbbreviation(
                    threshold: 1_000_000_000_000,
                    divisor: 1_000_000_000_000.0,
                    suffix: "T",
                    formatter: abbreviationFormatter
                )
            ])
        }

        return BigNumberFormatter(
            abbreviations: abbreviations,
            precision: Int(preferredPrecision),
            minimumFractionDigits: Int(minimumFractionDigits),
            rounding: roundingMode,
            usesIntGrouping: usesIntGrouping ?? true
        )
    }
}

extension AssetBalanceFormatterFactory: AssetBalanceFormatterFactoryProtocol {
    private func createInputNumberFormatter() -> NumberFormatter {
        let numberFormatter = NumberFormatter()
        numberFormatter.numberStyle = .decimal
        numberFormatter.minimumIntegerDigits = 1
        numberFormatter.usesGroupingSeparator = true
        numberFormatter.roundingMode = .down
        numberFormatter.alwaysShowsDecimalSeparator = false
        return numberFormatter
    }

    func createInputFormatter(
        for info: AssetBalanceDisplayInfo
    ) -> LocalizableResource<NumberFormatter> {
        let formatter = createInputNumberFormatter()
        formatter.maximumFractionDigits = Int(info.displayPrecision)
        return formatter.localizableResource()
    }

    func createTokenFormatter(
        for info: AssetBalanceDisplayInfo,
        roundingMode: NumberFormatter.RoundingMode
    ) -> LocalizableResource<LocalizableDecimalFormatting> {
        createTokenFormatterCommon(
            for: info,
            minimumFractionDigits: 0,
            roundingMode: roundingMode
        )
    }

    func createAssetPriceFormatter(
        for info: AssetBalanceDisplayInfo,
        minimumFractionDigits: UInt16
    ) -> LocalizableResource<LocalizableDecimalFormatting> {
        createTokenFormatterCommon(
            for: info,
            minimumFractionDigits: minimumFractionDigits,
            roundingMode: .down,
            preferredPrecisionOffset: 2
        )
    }
}
