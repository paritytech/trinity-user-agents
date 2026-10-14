import Foundation

public enum AttachmentSelectionViewModel {
    case image(ImageViewModelProtocol)
    case video(URL)
}
