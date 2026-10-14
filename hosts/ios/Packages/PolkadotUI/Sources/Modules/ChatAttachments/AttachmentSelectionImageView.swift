import UIKit

public final class AttachmentSelectionImageView: UIImageView {
    private var imageViewModel: ImageViewModelProtocol?

    public func bind(imageViewModel: ImageViewModelProtocol?) {
        imageViewModel?.cancel(on: self)

        self.imageViewModel = imageViewModel

        imageViewModel?.loadImage(on: self, settings: .originalImage, animated: true, completion: nil)
    }
}
