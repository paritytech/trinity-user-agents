import AVFoundation
import Foundation
import PolkadotUI
import UIKit

extension ChatImageLoader {
    /// Loads a v1 product media source, an inline `data:` image or an `https` URL, through the resolver.
    static func resources(_ resolver: ChatResourceResolving = ChatResourceResolver.shared) -> ChatImageLoader {
        ChatImageLoader { source in
            guard let resource = ChatResource(source: source) else { return nil }
            return await resolver.image(for: resource)
        }
    }

    /// Loads the poster of a video source: the first frame of the video (02-DESIGN §6.2). Gives nil when the bytes
    /// do not load, so the bubble offers to try again. A video whose frame the system cannot read gets a plain
    /// poster, so the user can still play it.
    static func videoPosters(_ resolver: ChatResourceResolving = ChatResourceResolver.shared) -> ChatImageLoader {
        ChatImageLoader { source in
            let request = ProductMediaOpenRequest(
                source: source,
                info: .init(kind: .video, mimeType: "", fileName: nil, sizeBytes: nil)
            )
            guard let url = await ProductMediaFiles(resolver: resolver).file(for: request) else {
                return nil
            }

            let generator = AVAssetImageGenerator(asset: AVURLAsset(url: url))
            generator.appliesPreferredTrackTransform = true
            generator.maximumSize = CGSize(width: 800, height: 800)
            if let frame = try? await generator.image(at: .zero).image {
                return UIImage(cgImage: frame)
            }
            return plainPoster
        }
    }

    private static let plainPoster = UIGraphicsImageRenderer(size: CGSize(width: 4, height: 4)).image { context in
        UIColor.darkGray.setFill()
        context.fill(CGRect(x: 0, y: 0, width: 4, height: 4))
    }
}
