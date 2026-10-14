import DesignSystem
import SwiftUI

public struct AvatarColorPalette {
    public let background: Color
    public let text: Color

    public init(background: Color, text: Color) {
        self.background = background
        self.text = text
    }
}

public extension AvatarColorPalette {
    static let allColors: [AvatarColorPalette] = [
        AvatarColorPalette(background: .avatarBgAmethyst, text: .avatarFgAmethyst),
        AvatarColorPalette(background: .avatarBgOpal, text: .avatarFgOpal),
        AvatarColorPalette(background: .avatarBgTurquoise, text: .avatarFgTurquoise),
        AvatarColorPalette(background: .avatarBgOnyx, text: .avatarFgOnyx),
        AvatarColorPalette(background: .avatarBgPearl, text: .avatarFgPearl),
        AvatarColorPalette(background: .avatarBgEmerald, text: .avatarFgEmerald),
        AvatarColorPalette(background: .avatarBgTopaz, text: .avatarFgTopaz),
        AvatarColorPalette(background: .avatarBgRuby, text: .avatarFgRuby),
        AvatarColorPalette(background: .avatarBgSapphire, text: .avatarFgSapphire),
        AvatarColorPalette(background: .avatarBgGarnet, text: .avatarFgGarnet)
    ]

    static func color(for seed: String) -> AvatarColorPalette {
        let sum = seed.utf8.reduce(0) { $0 + Int($1) }
        let index = sum % allColors.count
        return allColors[index]
    }
}
