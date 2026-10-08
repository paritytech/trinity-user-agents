import Foundation

// Builds DSAvatar configured for each place an avatar appears. Sizes come from Figma.
public enum DSAvatarFactory {
    // Chat list row.
    public static func chatList(_ viewModel: AvatarViewModel) -> DSAvatar {
        DSAvatar(viewModel: viewModel, size: .s64)
    }
}
