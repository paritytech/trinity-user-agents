import DesignSystem
import SwiftUI

/// A letter avatar with an asynchronously loaded image over it. The letter renders immediately and
/// stays visible for as long as the image is loading, or forever if it never arrives.
public struct DSAsyncAvatar: View {
    private let placeholder: AvatarViewModel
    private let icon: (any ImageViewModelProtocol)?
    private let size: DSLetterAvatar.Size

    public init(
        placeholder: AvatarViewModel,
        icon: (any ImageViewModelProtocol)?,
        size: DSLetterAvatar.Size
    ) {
        self.placeholder = placeholder
        self.icon = icon
        self.size = size
    }

    public var body: some View {
        DSAvatar(viewModel: placeholder, size: size)
            .overlay {
                if let icon {
                    AsyncImageView(
                        viewModel: icon,
                        settings: ImageViewModelSettings(targetSize: dimensions)
                    )
                    // The frame belongs on the representable: a UIImageView sizes itself from the
                    // image it loaded, so without it layout depends on what the processor produced.
                    .frame(width: dimensions.width, height: dimensions.height)
                    .clipShape(Circle())
                }
            }
    }
}

private extension DSAsyncAvatar {
    var dimensions: CGSize {
        CGSize(width: size.dimension, height: size.dimension)
    }
}
