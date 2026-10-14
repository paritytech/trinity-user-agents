import Foundation
import UIKit
import UniformTypeIdentifiers
import BlurHash

enum PHImageAttachmentProviderError: Error {
    case failedToLoadImage
    case downsamplingFailed
    case compressionFailed
    case fileSaveFailed
}

final class PHImageAttachmentProvider: @unchecked Sendable {
    let itemProvider: NSItemProvider

    private let previewSize: CGFloat = 600
    private let uploadSize: CGFloat = 4_800
    private let compressionQuality: CGFloat = 0.7
    private let logger: LoggerProtocol

    init(itemProvider: NSItemProvider, logger: LoggerProtocol = Logger.shared) {
        self.itemProvider = itemProvider
        self.logger = logger
    }
}

private extension PHImageAttachmentProvider {
    func processAttachment(from url: URL, store: AttachmentStoring) throws -> ProcessedAttachment {
        guard
            let imageToUpload = UIImage.downsampleImage(
                at: url,
                maxSideSize: uploadSize,
                scale: 1
            )
        else {
            throw PHImageAttachmentProviderError.downsamplingFailed
        }

        guard let uploadData = imageToUpload.jpegData(compressionQuality: compressionQuality) else {
            throw PHImageAttachmentProviderError.compressionFailed
        }

        let ext = url.pathExtension

        guard let fileName = (UUID().uuidString as NSString).appendingPathExtension(ext) else {
            throw PHImageAttachmentProviderError.fileSaveFailed
        }

        try store.store(attachment: uploadData, filename: fileName)

        let fileUrl = store.fileURL(for: fileName)
        let blurHash = makeBlurHash(from: url)

        let imageMeta = ChatRemoteMessageContent.ImageFileMeta(
            general: .init(
                mimeType: AttachmentMimeType.jpegImage,
                fileSize: UInt32(uploadData.count)
            ),
            width: UInt32(imageToUpload.size.width * imageToUpload.scale),
            height: UInt32(imageToUpload.size.height * imageToUpload.scale),
            thumbnail: blurHash?.toData()
        )

        return ProcessedAttachment(
            fileId: fileName,
            fileUrl: fileUrl,
            meta: .image(imageMeta)
        )
    }

    func makeBlurHash(from url: URL) -> BlurHash? {
        if let blurHashImage = UIImage.downsampleImage(
            at: url,
            maxSideSize: BlurHashConfiguration.encodingMaximumSide,
            scale: 1
        ), let value = blurHashImage.blurHash(numberOfComponents: BlurHashConfiguration.components),
        let encodedBlurHash = BlurHash(value) {
            return encodedBlurHash
        }

        logger.warning("Blur hash generation failed")
        return nil
    }
}

extension PHImageAttachmentProvider: ChatAttachmentProviding {
    var neededAudioActivity: AudioSessionActivity? {
        nil
    }

    func prepareForSend(using store: AttachmentStoring) async throws -> ProcessedAttachment {
        try await withCheckedThrowingContinuation { continuation in
            itemProvider.loadFileRepresentation(
                forTypeIdentifier: UTType.image.identifier
            ) { [weak self, store] url, error in
                if let error {
                    continuation.resume(throwing: error)
                    return
                }

                guard let sourceUrl = url, let self else {
                    continuation.resume(throwing: PHImageAttachmentProviderError.failedToLoadImage)
                    return
                }

                do {
                    let attachment = try processAttachment(
                        from: sourceUrl,
                        store: store
                    )

                    continuation.resume(returning: attachment)
                } catch {
                    continuation.resume(throwing: error)
                }
            }
        }
    }
}
