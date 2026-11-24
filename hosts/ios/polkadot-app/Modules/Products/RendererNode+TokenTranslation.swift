import BigInt
import PolkadotUI
import Products
import SwiftUI
import TrUAPIHost

// MARK: - Token translation

//
// The design-token resolver is defined over the SCALE token enums, so the core's
// leaf enums are translated into those rather than duplicating the resolver.

extension TrUAPIHostShape {
    func toScale() -> ScaleShape {
        switch self {
        case let .rounded(radius): .rounded(BigUInt(radius))
        case .square: .rounded(0)
        case .circle: .circle
        }
    }
}

extension ColorToken {
    func toScale() -> ScaleColorToken {
        switch self {
        case .fgPrimary: .textPrimary
        case .fgSecondary: .textSecondary
        case .fgTertiary: .textTertiary
        case .bgSurfaceMain: .backgroundPrimary
        case .bgSurfaceContainer: .backgroundSecondary
        case .bgSurfaceNested: .backgroundTertiary
        case .fgSuccess: .success
        case .fgError: .error
        case .fgWarning: .warning
        }
    }
}

extension TypographyStyle {
    func toScale() -> ScaleTypographyStyle {
        switch self {
        case .headlineLarge: .titleXL
        case .titleMediumRegular: .headline
        case .bodyLargeRegular: .bodyM
        case .bodyMediumRegular: .bodyS
        case .bodySmallRegular: .caption
        }
    }
}

extension ButtonVariant {
    func toScale() -> ScaleButtonVariant {
        switch self {
        case .primary: .primary
        case .secondary: .secondary
        case .text: .text
        }
    }
}

extension ContentAlignment {
    func toScale() -> ScaleContentAlignment {
        switch self {
        case .topStart: .topStart
        case .topCenter: .topCenter
        case .topEnd: .topEnd
        case .centerStart: .centerStart
        case .center: .center
        case .centerEnd: .centerEnd
        case .bottomStart: .bottomStart
        case .bottomCenter: .bottomCenter
        case .bottomEnd: .bottomEnd
        }
    }
}

extension TrUAPIHostHorizontalAlignment {
    func toScale() -> ScaleHorizontalAlignment {
        switch self {
        case .start: .start
        case .center: .center
        case .end: .end
        }
    }
}

extension TrUAPIHostVerticalAlignment {
    func toScale() -> ScaleVerticalAlignment {
        switch self {
        case .top: .top
        case .center: .center
        case .bottom: .bottom
        }
    }
}

extension Arrangement {
    func toScale() -> ScaleArrangement {
        switch self {
        case .start: .start
        case .end: .end
        case .center: .center
        case .spaceBetween: .spaceBetween
        case .spaceAround: .spaceAround
        case .spaceEvenly: .spaceEvenly
        }
    }
}

extension BlendingMode {
    var blendMode: BlendMode {
        switch self {
        case .normal: .normal
        case .multiply: .multiply
        case .screen: .screen
        case .overlay: .overlay
        case .darken: .darken
        case .lighten: .lighten
        case .colorDodge: .colorDodge
        case .colorBurn: .colorBurn
        case .hardLight: .hardLight
        case .softLight: .softLight
        case .difference: .difference
        case .exclusion: .exclusion
        case .hue: .hue
        case .saturation: .saturation
        case .color: .color
        case .luminosity: .luminosity
        }
    }
}
