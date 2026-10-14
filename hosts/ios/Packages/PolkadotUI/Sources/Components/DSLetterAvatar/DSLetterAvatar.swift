import DesignSystem
import SwiftUI

public struct DSLetterAvatar: View, Equatable {
    public enum Size: Hashable {
        case s20
        case s28
        case s40
        case s44
        case s48
        case s56
        case s64
        case s72
        case s108
        case s136
    }

    private let letter: String
    private let background: Color
    private let foreground: Color
    private let size: Size

    public init(
        letter: String,
        background: Color,
        foreground: Color,
        size: Size = .s64
    ) {
        self.letter = letter
        self.background = background
        self.foreground = foreground
        self.size = size
    }

    public var body: some View {
        Text(letter)
            .typography(size.typography)
            .foregroundStyle(foreground)
            .lineLimit(1)
            .frame(width: size.dimension, height: size.dimension)
            .background(background, in: Circle())
    }
}

public extension DSLetterAvatar.Size {
    var dimension: CGFloat {
        switch self {
        case .s20: 20
        case .s28: 28
        case .s40: 40
        case .s44: 44
        case .s48: 48
        case .s56: 56
        case .s64: 64
        case .s72: 72
        case .s108: 108
        case .s136: 136
        }
    }
}

private extension DSLetterAvatar.Size {
    var typography: TypographyStyle {
        switch self {
        case .s20: .labelMedium.emphasized
        case .s28: .titleExtraLarge
        case .s40: .headlineSmall
        case .s44,
             .s48: .headlineMedium
        case .s56,
             .s64: .headlineLarge
        case .s72: .displaySmall
        case .s108: .displayLarge
        case .s136: .displayExtraLarge
        }
    }
}

#if DEBUG
    #Preview("Sizes — amethyst") {
        let sizes: [DSLetterAvatar.Size] = [.s20, .s28, .s40, .s44, .s48, .s56, .s64, .s72, .s108, .s136]
        return VStack(alignment: .leading, spacing: 12) {
            ForEach(sizes, id: \.self) { size in
                HStack(spacing: 12) {
                    DSLetterAvatar(
                        letter: "A",
                        background: .avatarBgAmethyst,
                        foreground: .avatarFgAmethyst,
                        size: size
                    )
                    Text("\(Int(size.dimension)) pt")
                        .typography(.bodyMedium)
                        .foregroundStyle(Color.fgSecondary)
                }
            }
        }
        .padding(24)
        .background(Color.bgSurfaceMain)
    }

    #Preview("Palettes — 64pt") {
        let palettes: [(letter: String, bg: Color, fg: Color)] = [
            ("A", .avatarBgAmethyst, .avatarFgAmethyst),
            ("O", .avatarBgOpal, .avatarFgOpal),
            ("T", .avatarBgTurquoise, .avatarFgTurquoise),
            ("N", .avatarBgOnyx, .avatarFgOnyx),
            ("P", .avatarBgPearl, .avatarFgPearl),
            ("E", .avatarBgEmerald, .avatarFgEmerald),
            ("Z", .avatarBgTopaz, .avatarFgTopaz),
            ("R", .avatarBgRuby, .avatarFgRuby),
            ("S", .avatarBgSapphire, .avatarFgSapphire),
            ("G", .avatarBgGarnet, .avatarFgGarnet)
        ]
        return LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 12), count: 5), spacing: 12) {
            ForEach(palettes, id: \.letter) { palette in
                DSLetterAvatar(
                    letter: palette.letter,
                    background: palette.bg,
                    foreground: palette.fg,
                    size: .s64
                )
            }
        }
        .padding(24)
        .background(Color.bgSurfaceMain)
    }
#endif
