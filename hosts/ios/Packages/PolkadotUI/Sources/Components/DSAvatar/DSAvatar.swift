import DesignSystem
import SwiftUI

public struct DSAvatar: View, Equatable {
    private let viewModel: AvatarViewModel
    private let size: DSLetterAvatar.Size

    public init(viewModel: AvatarViewModel, size: DSLetterAvatar.Size) {
        self.viewModel = viewModel
        self.size = size
    }

    public var body: some View {
        if let image = viewModel.image {
            Image(uiImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: size.dimension, height: size.dimension)
                .clipShape(Circle())
        } else {
            DSLetterAvatar(
                letter: viewModel.text ?? "",
                background: viewModel.backgroundColor.map { Color($0) } ?? .avatarBgAmethyst,
                foreground: viewModel.textColor.map { Color($0) } ?? .avatarFgAmethyst,
                size: size
            )
        }
    }
}

public extension DSAvatar {
    static func placeholder(of size: DSLetterAvatar.Size) -> DSAvatar {
        DSAvatar(viewModel: .colored(text: "", colorSeed: ""), size: size)
    }
}
