import UIKit
import QuickLookThumbnailing

public class PreviewImageViewModel {
    let url: URL

    private var task: Task<Void, Error>?

    public init(url: URL) {
        self.url = url
    }
}

extension PreviewImageViewModel: Hashable {
    public static func == (lhs: PreviewImageViewModel, rhs: PreviewImageViewModel) -> Bool {
        lhs.url == rhs.url
    }

    public func hash(into hasher: inout Hasher) {
        hasher.combine(url)
    }
}

extension PreviewImageViewModel: ImageViewModelProtocol {
    public func loadImage(
        on imageView: UIImageView,
        settings: ImageViewModelSettings,
        animated _: Bool,
        completion: ((Bool) -> Void)?
    ) {
        task?.cancel()
        task = Task {
            do {
                let image = try await filePreview(
                    at: url,
                    targetSize: settings.targetSize
                )
                try Task.checkCancellation()
                updateImageView(imageView, with: image)
                completion?(true)
            } catch {
                completion?(false)
            }
        }
    }

    public func cancel(on imageView: UIImageView) {
        task?.cancel()
        imageView.image = nil
    }
}

private extension PreviewImageViewModel {
    func filePreview(at url: URL, targetSize: CGSize?) async throws -> UIImage {
        let request = await QLThumbnailGenerator.Request(
            fileAt: url,
            size: targetSize ?? CGSize(width: 150, height: 150),
            scale: UIScreen.main.scale,
            representationTypes: .thumbnail
        )
        return try await QLThumbnailGenerator.shared
            .generateBestRepresentation(for: request)
            .uiImage
    }

    @MainActor
    func updateImageView(_ imageView: UIImageView, with image: UIImage?) {
        imageView.image = image
    }
}
