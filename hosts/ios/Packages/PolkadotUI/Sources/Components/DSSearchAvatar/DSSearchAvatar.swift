import DesignSystem
import SwiftUI

/// Search result avatar: the contact's image in a 40 pt circle, or its initial as a `DSLetterAvatar`.
public struct DSSearchAvatar: View, Equatable {
    public static let dimension: CGFloat = 40

    private let viewModel: AvatarViewModel

    public init(viewModel: AvatarViewModel) {
        self.viewModel = viewModel
    }

    public var body: some View {
        if let image = viewModel.image {
            Image(uiImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: Self.dimension, height: Self.dimension)
                .clipShape(Circle())
        } else {
            DSLetterAvatar(
                letter: viewModel.text ?? "",
                background: viewModel.backgroundColor.map { Color($0) } ?? .avatarBgAmethyst,
                foreground: viewModel.textColor.map { Color($0) } ?? .avatarFgAmethyst,
                size: .s40
            )
        }
    }
}
