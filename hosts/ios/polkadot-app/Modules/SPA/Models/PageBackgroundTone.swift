import DesignSystem
import UIKit

/// Whether a product page draws on a dark or a light background, read from the CSS colours the page computes.
enum PageBackgroundTone: Equatable {
    case dark
    case light

    /// The tone of the first colour in `cssColors` that is not fully transparent, or nil when there is none.
    ///
    /// Each colour is a computed CSS value such as `rgb(5, 5, 5)` or `rgba(0, 0, 0, 0)`, outermost element first,
    /// so a page that paints `body` over a transparent `html` reads as its `body`.
    init?(cssColors: [String]) {
        guard let color = Self.visibleColor(in: cssColors) else { return nil }

        self.init(color: color)
    }

    /// The tone of an opaque colour, from its Rec. 709 luma against the dark threshold.
    init(color: UIColor) {
        var red: CGFloat = 0
        var green: CGFloat = 0
        var blue: CGFloat = 0
        color.getRed(&red, green: &green, blue: &blue, alpha: nil)

        let luminance = Self.redWeight * Double(red) + Self.greenWeight * Double(green) + Self.blueWeight * Double(blue)
        self = luminance < Self.darkThreshold ? .dark : .light
    }

    /// The first colour in `cssColors` that is not fully transparent, as an opaque colour.
    ///
    /// The colours are computed CSS values, outermost element first. Nil when every colour is fully transparent.
    static func visibleColor(in cssColors: [String]) -> UIColor? {
        cssColors.lazy.compactMap { Self.visibleColor(of: $0) }.first
    }

    /// The theme the status strip takes over a page of this tone, or nil where the app's own theme already reads
    /// on it. Only a clash is overridden, so the strip keeps the colours of a light or dark theme other than the
    /// two named here.
    func statusStripTheme(appThemeIsLight: Bool) -> ThemeSelection? {
        switch (self, appThemeIsLight) {
        case (.dark, true):
            .berlinNight
        case (.light, false):
            .berlinDay
        case (.dark, false),
             (.light, true):
            nil
        }
    }
}

private extension PageBackgroundTone {
    /// Rec. 709 luma weights, applied to the encoded sRGB values without linearising them.
    static let redWeight = 0.2126
    static let greenWeight = 0.7152
    static let blueWeight = 0.0722
    static let darkThreshold = 0.5

    /// An opaque colour from an `rgb()` or `rgba()` value, nil when fully transparent or in another notation.
    static func visibleColor(of cssColor: String) -> UIColor? {
        guard
            cssColor.hasPrefix("rgb"),
            let open = cssColor.firstIndex(of: "("),
            let close = cssColor.lastIndex(of: ")"),
            open < close
        else {
            return nil
        }

        let numbers = cssColor[cssColor.index(after: open) ..< close]
            .split(whereSeparator: { $0 == "," || $0 == " " || $0 == "/" })
            .compactMap { Double($0) }

        guard numbers.count >= 3 else { return nil }

        if numbers.count >= 4, numbers[3] == 0 {
            return nil
        }
        return UIColor(red: numbers[0] / 255, green: numbers[1] / 255, blue: numbers[2] / 255, alpha: 1)
    }
}
