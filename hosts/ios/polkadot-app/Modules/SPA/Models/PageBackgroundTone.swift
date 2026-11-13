import DesignSystem

/// Whether a product page draws on a dark or a light background, read from the CSS colours the page computes.
enum PageBackgroundTone: Equatable {
    case dark
    case light

    /// The tone of the first colour in `cssColors` that is not fully transparent, or nil when there is none.
    ///
    /// Each colour is a computed CSS value such as `rgb(5, 5, 5)` or `rgba(0, 0, 0, 0)`, outermost element first,
    /// so a page that paints `body` over a transparent `html` reads as its `body`.
    init?(cssColors: [String]) {
        guard let color = cssColors.lazy.compactMap({ Self.visibleComponents(of: $0) }).first else {
            return nil
        }

        let luminance = Self.redWeight * color.red + Self.greenWeight * color.green + Self.blueWeight * color.blue
        self = luminance < Self.darkThreshold ? .dark : .light
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

    /// Red, green and blue in 0...1 of an `rgb()` or `rgba()` value, or nil when it is fully transparent or in
    /// another notation.
    static func visibleComponents(of cssColor: String) -> (red: Double, green: Double, blue: Double)? {
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
        return (numbers[0] / 255, numbers[1] / 255, numbers[2] / 255)
    }
}
