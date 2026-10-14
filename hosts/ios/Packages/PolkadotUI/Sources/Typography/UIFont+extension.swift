import UIKit

// TODO: need a revision

public extension UIFont {
    static var regular12: UIFont {
        customFont(.interRegular, size: 12)
    }

    static var regular14: UIFont {
        customFont(.interRegular, size: 14)
    }

    static var regular16: UIFont {
        customFont(.interRegular, size: 16)
    }

    static var body14: UIFont {
        customFont(.interRegular, size: 14)
    }

    static var medium16: UIFont {
        customFont(.interMedium, size: 16)
    }

    static var semibold12: UIFont {
        customFont(.interSemiBold, size: 12)
    }

    static var semibold10: UIFont {
        customFont(.interSemiBold, size: 10)
    }

    static var semibold14: UIFont {
        customFont(.interSemiBold, size: 14)
    }

    static var semibold16: UIFont {
        customFont(.interSemiBold, size: 16)
    }

    static var semibold18: UIFont {
        customFont(.interSemiBold, size: 18)
    }

    static var semibold20: UIFont {
        customFont(.interSemiBold, size: 20)
    }

    static var semibold24: UIFont {
        customFont(.interSemiBold, size: 24)
    }

    static var semibold32: UIFont {
        customFont(.interSemiBold, size: 32)
    }

    static var semibold48: UIFont {
        customFont(.interSemiBold, size: 48)
    }

    static var semibold56: UIFont {
        customFont(.interSemiBold, size: 56)
    }

    static var semiBold14: UIFont {
        customFont(.interSemiBold, size: 14)
    }

    static var bold16: UIFont {
        customFont(.interBold, size: 16)
    }

    static var bold18: UIFont {
        customFont(.interBold, size: 18)
    }

    static func body14Regular() -> UIFont {
        customFont(.interRegular, size: 14.0)
    }

    static func body16Regular() -> UIFont {
        customFont(.interRegular, size: 16.0)
    }

    static func caption10Regular() -> UIFont {
        customFont(.interRegular, size: 10.0)
    }

    static func caption12Regular() -> UIFont {
        customFont(.interRegular, size: 12.0)
    }

    static func caption13Regular() -> UIFont {
        customFont(.interRegular, size: 13.0)
    }

    static func headline16Medium() -> UIFont {
        customFont(.interMedium, size: 16.0)
    }

    static func title14SemiBold() -> UIFont {
        customFont(.interSemiBold, size: 14.0)
    }

    static func title24SemiBold() -> UIFont {
        customFont(.interSemiBold, size: 24.0)
    }

    static func title18SemiBold() -> UIFont {
        customFont(.interSemiBold, size: 18.0)
    }

    static func title16Medium() -> UIFont {
        customFont(.interMedium, size: 16.0)
    }

    static func title16SemiBold() -> UIFont {
        customFont(.interSemiBold, size: 16.0)
    }

    static func title32SemiBold() -> UIFont {
        customFont(.interSemiBold, size: 32.0)
    }

    static func title48SemiBold() -> UIFont {
        customFont(.interSemiBold, size: 48.0)
    }

    static func title56SemiBold() -> UIFont {
        customFont(.interSemiBold, size: 56.0)
    }

    static func headlineMulishXL() -> UIFont {
        customFont(.mulishBlack, size: 40.0)
    }

    static func buttonMulishExtraBlack() -> UIFont {
        customFont(.mulishExtraBlack, size: 22.0)
    }

    static func titleMulish32ExtraBlack() -> UIFont {
        customFont(.mulishExtraBlack, size: 32.0)
    }

    static func titleMulish164ExtraBlack() -> UIFont {
        customFont(.mulishExtraBlack, size: 164.0)
    }

    private static func customFont(
        _ font: CustomFont,
        size: CGFloat,
        textStyle: UIFont.TextStyle? = nil,
        scaled: Bool = false
    ) -> UIFont {
        registerAllFontsIfNeeded()
        guard
            let font = UIFont(name: font.name, size: size)
        else {
            print("Warning: Font \(font.name) not found.")
            assertionFailure()
            return UIFont.systemFont(ofSize: size, weight: .regular)
        }

        guard scaled, let textStyle else {
            return font
        }

        let metrics = UIFontMetrics(forTextStyle: textStyle)
        return metrics.scaledFont(for: font)
    }
}

// Font registration
extension UIFont {
    static var fontsRegistered: Bool = false

    static func registerAllFontsIfNeeded() {
        guard !fontsRegistered else { return }
        let bundle = Bundle.module
        let urls = bundle.allFontURLs()

        CTFontManagerRegisterFontURLs(urls as CFArray, .process, true, nil)

        fontsRegistered = true
    }
}

public extension UIFont {
    func monospaced() -> UIFont {
        let fontDescriptor = fontDescriptor.addingAttributes([
            .featureSettings: [
                [
                    UIFontDescriptor.FeatureKey.type: kNumberSpacingType,
                    UIFontDescriptor.FeatureKey.selector: kMonospacedNumbersSelector
                ]
            ]
        ])
        return UIFont(descriptor: fontDescriptor, size: pointSize)
    }
}

enum CustomFont: String {
    case interSemiBold = "Inter-SemiBold"
    case interRegular = "Inter-Regular"
    case interMedium = "Inter-Medium"
    case interBold = "Inter-Bold"
    case mulishBlack = "MulishRoman-Black"
    case mulishExtraBlack = "MulishRoman-ExtraBlack"

    var name: String {
        rawValue
    }
}

private extension Bundle {
    func allFontURLs() -> [URL] {
        guard let resourceURL,
              let enumerator = FileManager.default.enumerator(
                  at: resourceURL,
                  includingPropertiesForKeys: nil,
                  options: [.skipsHiddenFiles]
              ) else {
            return []
        }
        var results: [URL] = []
        for case let fileURL as URL in enumerator {
            let ext = fileURL.pathExtension.lowercased()
            guard ext == "ttf" else {
                continue
            }
            results.append(fileURL)
        }
        return results
    }
}
