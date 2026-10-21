import AVFoundation
import Foundation

/// Makes a video that is larger than the inline limit smaller for a product room: the host exports it at 640x480,
/// as the native person chat sends a video (`AVAssetExportPreset640x480`). A video that is still too large after
/// the export is refused, because a room without staging carries at most 1 MiB inline.
enum ProductVideoCompressor {
    enum Failure: Error, Equatable {
        /// The video at 640x480 is still larger than the limit.
        case tooLarge(bytes: Int, limit: Int)
        /// The video could not be read or exported.
        case unreadable
    }

    /// The export preset: the preset of the native person chat.
    static let preset = AVAssetExportPreset640x480

    /// The video `data` as an MP4 at 640x480 of at most `limit` bytes.
    static func compress(_ data: Data, limit: Int) async throws -> Data {
        let folder = FileManager.default.temporaryDirectory.appending(
            path: "ProductVideo-\(UUID().uuidString)",
            directoryHint: .isDirectory
        )
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: folder) }

        let input = folder.appending(path: "input.mov")
        let output = folder.appending(path: "output.mp4")
        try data.write(to: input)

        guard let session = AVAssetExportSession(asset: AVURLAsset(url: input), presetName: preset) else {
            throw Failure.unreadable
        }
        session.shouldOptimizeForNetworkUse = true
        session.metadataItemFilter = AVMetadataItemFilter.forSharing()

        do {
            try await session.export(to: output, as: .mp4)
        } catch {
            throw Failure.unreadable
        }

        let exported = try Data(contentsOf: output)
        guard exported.count <= limit else {
            throw Failure.tooLarge(bytes: exported.count, limit: limit)
        }
        return exported
    }
}
