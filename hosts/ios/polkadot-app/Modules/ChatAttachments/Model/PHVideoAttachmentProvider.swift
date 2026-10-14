import Foundation
import Photos
import UIKit
import UniformTypeIdentifiers
import BlurHash

enum PHVideoAttachmentProviderError: Error {
    case videoNotFound
    case exportSessionFailed
    case noSupportedFileTypes
    case fileSizeQueryFailed
}

final class PHVideoAttachmentProvider: @unchecked Sendable {
    let itemProvider: NSItemProvider
    let logger: LoggerProtocol

    private let exportPreset: String = AVAssetExportPreset640x480

    init(itemProvider: NSItemProvider, logger: LoggerProtocol = Logger.shared) {
        self.itemProvider = itemProvider
        self.logger = logger
    }
}

private extension PHVideoAttachmentProvider {
    func makeBlurHash(for outputURL: URL) async -> Data? {
        do {
            let generator = AVAssetImageGenerator(asset: AVURLAsset(url: outputURL))
            generator.appliesPreferredTrackTransform = true
            generator.maximumSize = BlurHashConfiguration.encodingPreviewSize
            let cgImage = try await generator.image(at: .zero).image

            return UIImage(cgImage: cgImage)
                .blurHash(numberOfComponents: BlurHashConfiguration.components)
                .flatMap { BlurHash($0) }?
                .toData()
        } catch {
            logger.warning("Blur hash generation failed: \(error)")
            return nil
        }
    }
}

extension PHVideoAttachmentProvider: ChatAttachmentProviding {
    var neededAudioActivity: AudioSessionActivity? {
        .playback
    }

    func prepareForSend(using store: AttachmentStoring) async throws -> ProcessedAttachment {
        logger.debug("Video processing did start")

        guard
            let tmpUrl = try await itemProvider.moveRepresentationToTempDirectory(
                forTypeIdentifier: UTType.movie.identifier
            )
        else {
            throw PHVideoAttachmentProviderError.videoNotFound
        }

        // delete tmp file once processed
        defer {
            try? FileManager.default.removeItem(at: tmpUrl)
        }

        logger.debug("Video moved to tmp path to start export")

        let avAsset = AVURLAsset(url: tmpUrl)

        let duration = try await avAsset.load(.duration)

        guard let exportSession = AVAssetExportSession(
            asset: avAsset,
            presetName: exportPreset
        ) else {
            throw PHVideoAttachmentProviderError.exportSessionFailed
        }

        exportSession.shouldOptimizeForNetworkUse = true
        exportSession.metadataItemFilter = AVMetadataItemFilter.forSharing()

        let supportedFileTypes = exportSession.supportedFileTypes
        guard
            let videoType = supportedFileTypes.contains(.mp4) ? .mp4 : supportedFileTypes.first,
            let utType = UTType(videoType.rawValue) else {
            throw PHVideoAttachmentProviderError.noSupportedFileTypes
        }

        var fileName = UUID().uuidString

        if
            let fileExtension = utType.preferredFilenameExtension,
            let newFileName = (fileName as NSString).appendingPathExtension(fileExtension) {
            fileName = newFileName
        }

        try store.createDirectoryIfNeeded()
        let outputURL = store.fileURL(for: fileName)

        logger.debug("Video export started")

        try await exportSession.export(to: outputURL, as: videoType)

        logger.debug("Video export finished")

        let values = try outputURL.resourceValues(forKeys: [.fileSizeKey])

        guard let fileSize = values.fileSize else {
            throw PHVideoAttachmentProviderError.fileSizeQueryFailed
        }

        let thumbnail = await makeBlurHash(for: outputURL)

        logger.debug("Video final size: \(fileSize)")

        let videoMeta = ChatRemoteMessageContent.VideoFileMeta(
            general: .init(
                mimeType: utType.preferredMIMEType ?? AttachmentMimeType.defaultVideo,
                fileSize: UInt32(fileSize)
            ),
            duration: UInt32(duration.seconds),
            thumbnail: thumbnail
        )

        return ProcessedAttachment(
            fileId: fileName,
            fileUrl: outputURL,
            meta: .video(videoMeta)
        )
    }
}
