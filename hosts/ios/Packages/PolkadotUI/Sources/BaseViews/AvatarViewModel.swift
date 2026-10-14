import UIKit
import SwiftUI

public struct AvatarViewModel: Hashable {
    public let backgroundColor: UIColor?
    public let image: UIImage?
    public let textColor: UIColor?
    public let text: String?

    public static func colored(text: String, colorSeed: String) -> AvatarViewModel {
        let palette = AvatarColorPalette.color(for: colorSeed)
        return AvatarViewModel(
            backgroundColor: UIColor(palette.background),
            image: nil,
            textColor: UIColor(palette.text),
            text: text.uppercased()
        )
    }

    public static func image(_ image: UIImage) -> AvatarViewModel {
        AvatarViewModel(
            backgroundColor: nil,
            image: image,
            textColor: nil,
            text: nil
        )
    }

    private init(
        backgroundColor: UIColor?,
        image: UIImage?,
        textColor: UIColor?,
        text: String?
    ) {
        self.backgroundColor = backgroundColor
        self.image = image
        self.textColor = textColor
        self.text = text
    }
}
