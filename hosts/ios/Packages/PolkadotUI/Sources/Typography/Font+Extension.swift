import SwiftUI

// TODO: need a revision

public extension Font {
    static var regular12: Font {
        customFont(.interRegular, size: 12)
    }

    static var regular14: Font {
        customFont(.interRegular, size: 14)
    }

    static var regular16: Font {
        customFont(.interRegular, size: 16)
    }

    static var body14: Font {
        customFont(.interRegular, size: 14)
    }

    static var medium16: Font {
        customFont(.interMedium, size: 16)
    }

    static var semibold14: Font {
        customFont(.interSemiBold, size: 14)
    }

    static var semibold16: Font {
        customFont(.interSemiBold, size: 16)
    }

    static var semibold18: Font {
        customFont(.interSemiBold, size: 18)
    }

    static var semibold24: Font {
        customFont(.interSemiBold, size: 24)
    }

    static var semibold32: Font {
        customFont(.interSemiBold, size: 32)
    }

    static var semibold48: Font {
        customFont(.interSemiBold, size: 48)
    }

    static func body14Regular() -> Font {
        customFont(.interRegular, size: 14.0)
    }

    static func body16Regular() -> Font {
        customFont(.interRegular, size: 16.0)
    }

    static func body22Regular() -> Font {
        customFont(.interRegular, size: 22.0)
    }

    static func caption10Regular() -> Font {
        customFont(.interRegular, size: 10.0)
    }

    static func caption12Regular() -> Font {
        customFont(.interRegular, size: 12.0)
    }

    static func headline16Medium() -> Font {
        customFont(.interMedium, size: 16.0)
    }

    static func title24SemiBold() -> Font {
        customFont(.interSemiBold, size: 24.0)
    }

    static func title18SemiBold() -> Font {
        customFont(.interSemiBold, size: 18.0)
    }

    static func title16Medium() -> Font {
        customFont(.interMedium, size: 16.0)
    }

    static func title14SemiBold() -> Font {
        customFont(.interSemiBold, size: 14.0)
    }

    static func title16SemiBold() -> Font {
        customFont(.interSemiBold, size: 16.0)
    }

    static func title32SemiBold() -> Font {
        customFont(.interSemiBold, size: 32.0)
    }

    static func title48SemiBold() -> Font {
        customFont(.interSemiBold, size: 48.0)
    }

    static func title56SemiBold() -> Font {
        customFont(.interSemiBold, size: 56.0)
    }

    static func headlineMulishXL() -> Font {
        customFont(.mulishBlack, size: 40.0)
    }

    static func titleMulish18Black() -> Font {
        customFont(.mulishBlack, size: 18.0)
    }

    static func buttonMulishExtraBlack() -> Font {
        customFont(.mulishExtraBlack, size: 22.0)
    }

    static func titleMulish32ExtraBlack() -> Font {
        customFont(.mulishExtraBlack, size: 32.0)
    }

    static func titleMulish164ExtraBlack() -> Font {
        customFont(.mulishExtraBlack, size: 164.0)
    }

    private static func customFont(
        _ font: CustomFont,
        size: CGFloat
    ) -> Font {
        UIFont.registerAllFontsIfNeeded()

        if let uiFont = UIFont(name: font.name, size: size) {
            return Font(uiFont)
        } else {
            print("Warning: Font \(font.name) not found.")
            assertionFailure()
            return Font.system(size: size, weight: .regular)
        }
    }
}
