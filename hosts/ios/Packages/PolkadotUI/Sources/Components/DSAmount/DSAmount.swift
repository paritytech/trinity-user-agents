import DesignSystem
import SwiftUI
import UIKit

/// A figure and the symbol of what it counts, set as one run of text.
///
/// The figure carries the fiat symbol in front of it and is formatted by whoever supplies it; this
/// only decides how the asset symbol after it is set. That symbol is deliberately quieter than the
/// figure, so the number reads first and the currency is there to be checked rather than announced.
///
/// Concatenated rather than laid out in a stack: the two sit on one baseline whatever sizes they
/// are given, which a stack only approximates.
public struct DSAmount: View {
    private let amount: String
    private let symbol: String?
    private let typography: TypographyStyle

    public init(amount: String, symbol: String?, typography: TypographyStyle) {
        self.amount = amount
        self.symbol = symbol
        self.typography = typography
    }

    public var body: some View {
        text.typography(typography)
    }
}

private extension DSAmount {
    /// The symbol is set in the small caps face at the figure's size, so it stays quieter than
    /// the figure through its lighter weight and lower glyph height rather than through scaling.
    static let symbolWeight: TypographyFontWeight = .light

    var text: Text {
        guard let symbol, !symbol.isEmpty else { return Text(amount) }

        return Text(amount) + Text(" \(symbol)").font(symbolFont)
    }

    var symbolFont: Font {
        let spec = typography.resolvedSpec
        let font = TypographyManager.shared.family.font(
            family: .smallCaps,
            weight: Self.symbolWeight,
            size: spec.size
        )

        return Font(font)
    }
}

#if DEBUG
    #Preview("DSAmount") {
        VStack(alignment: .leading, spacing: 16) {
            DSAmount(amount: "$15", symbol: "CASH", typography: .displaySmall)
            DSAmount(amount: "$1,234.56", symbol: "CASH", typography: .headlineMedium)
            DSAmount(amount: "$15", symbol: "CASH", typography: .titleLarge)
            DSAmount(amount: "$15", symbol: nil, typography: .titleLarge)
        }
        .foregroundStyle(Color.fgPrimary)
        .padding()
        .background(Color.bgSurfaceContainer)
    }
#endif
