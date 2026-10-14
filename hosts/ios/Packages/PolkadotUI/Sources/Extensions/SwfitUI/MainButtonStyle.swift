import DesignSystem
import SwiftUI

public struct MainButtonStyle: ButtonStyle {
    let backgroundColor: Color
    let foregroundColor: Color
    let disabledForegroundColor: Color
    let disabledBackgroundColor: Color
    let height: CGFloat

    init(
        backgroundColor: Color,
        foregroundColor: Color,
        disabledForegroundColor: Color,
        disabledBackgroundColor: Color? = nil,
        height: CGFloat
    ) {
        self.backgroundColor = backgroundColor
        self.foregroundColor = foregroundColor
        self.disabledForegroundColor = disabledForegroundColor
        self.disabledBackgroundColor = disabledBackgroundColor ?? backgroundColor
        self.height = height
    }

    @Environment(\.isEnabled) private var isEnabled

    public func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .frame(height: height)
            .background(isEnabled ? backgroundColor : disabledBackgroundColor, in: RoundedRectangle(cornerRadius: 12))
            .foregroundStyle(isEnabled ? foregroundColor : disabledForegroundColor)
            .opacity(configuration.isPressed ? 0.5 : 1)
    }
}

public extension MainButtonStyle {
    init(
        backgroundColor: Color,
        foregroundColor: Color,
        height: CGFloat = 56
    ) {
        self.backgroundColor = backgroundColor
        self.foregroundColor = foregroundColor
        disabledForegroundColor = foregroundColor
        disabledBackgroundColor = backgroundColor
        self.height = height
    }
}

public extension ButtonStyle where Self == MainButtonStyle {
    static var mainWhite: MainButtonStyle {
        MainButtonStyle(
            backgroundColor: Color.bgActionPrimary,
            foregroundColor: Color.fgPrimaryInverted,
            disabledForegroundColor: Color.fgPrimaryInverted.opacity(0.7),
            height: 56
        )
    }

    static var mainDark: MainButtonStyle {
        MainButtonStyle(
            backgroundColor: Color.bgActionTertiary,
            foregroundColor: Color.fgPrimary,
            disabledForegroundColor: Color.fgTertiary,
            height: 56
        )
    }

    static var mainDark44: MainButtonStyle {
        MainButtonStyle(
            backgroundColor: Color.bgActionTertiary,
            foregroundColor: Color.fgPrimary,
            disabledForegroundColor: Color.fgTertiary,
            height: 44
        )
    }

    static var mainWhite44: MainButtonStyle {
        MainButtonStyle(
            backgroundColor: Color.bgActionPrimary,
            foregroundColor: Color.fgPrimaryInverted,
            disabledForegroundColor: Color.fgTertiary,
            disabledBackgroundColor: Color.bgActionTertiary,
            height: 44
        )
    }

    static var destructiveDark44: MainButtonStyle {
        MainButtonStyle(
            backgroundColor: Color.bgActionTertiary,
            foregroundColor: Color.fgError,
            disabledForegroundColor: Color.fgError,
            height: 44
        )
    }
}
