import Foundation
import Darwin
import ImageIO
import AVFoundation
import UIKit
import UniformTypeIdentifiers
import BlurHash
import TrUAPIHost

/// Application Support attachment storage, like AttachmentStore's native Chat
/// uploads/downloads. Only canonical random IDs are ever used as filenames.
/// A single process-wide actor serializes reads, release and export writes.
actor TrUAPIChatFileStore {
    static let shared = TrUAPIChatFileStore()
    private let sources: AttachmentStoring?
    private let exports: AttachmentStoring?
    private var pendingExports: [String: Export] = [:]
    private static let mediaExtensions = [
        "image/jpeg": "jpg", "image/png": "png", "image/gif": "gif",
        "image/tiff": "tiff", "image/heic": "heic", "image/heif": "heif",
        "image/bmp": "bmp", "image/webp": "webp",
        "video/mp4": "mp4", "video/quicktime": "mov"
    ]

    private struct Export {
        let size: UInt64
        var written: UInt64 = 0
        var presenting = false
    }

    init(
        sources: AttachmentStoring? = AttachmentStore.attachmentsInDocument(directory: "TrUAPIChatSources"),
        exports: AttachmentStoring? = AttachmentStore.attachmentsInDocument(directory: "TrUAPIChatExports")
    ) {
        self.sources = sources
        self.exports = exports
    }

    func importFiles(_ urls: [URL]) async throws -> [NativeChatPickedFile] {
        let store = try prepare(sources)
        var imported: [NativeChatPickedFile] = []
        do {
            for url in urls {
                try Task.checkCancellation()
                let id = UUID().uuidString
                let destination = store.fileURL(for: id)
                let temporary = store.fileURL(for: id + ".pending")
                var committed = false
                defer {
                    try? FileManager.default.removeItem(at: temporary)
                    if !committed { try? FileManager.default.removeItem(at: destination) }
                }
                let scoped = url.startAccessingSecurityScopedResource()
                defer { if scoped { url.stopAccessingSecurityScopedResource() } }
                var coordinationError: NSError?
                var copyResult: Result<Void, Error>?
                NSFileCoordinator(filePresenter: nil).coordinate(
                    readingItemAt: url,
                    options: .withoutChanges,
                    error: &coordinationError
                ) { coordinatedURL in
                    copyResult = Result {
                        _ = try self.fileSize(coordinatedURL)
                        try FileManager.default.copyItem(at: coordinatedURL, to: temporary)
                    }
                }
                if let coordinationError { throw coordinationError }
                guard let copyResult else { throw ChatFileFailure.unavailable }
                try copyResult.get()
                let size = try fileSize(temporary)
                try FileManager.default.setAttributes([.posixPermissions: 0o600], ofItemAtPath: temporary.path)
                try protectAndSync(temporary)
                try FileManager.default.setAttributes([.posixPermissions: 0o400], ofItemAtPath: temporary.path)
                try FileManager.default.moveItem(at: temporary, to: destination)
                try syncDirectory(destination.deletingLastPathComponent())
                let metadata = await self.metadata(for: destination, size: size)
                imported.append(NativeChatPickedFile(sourceId: id, metadata: metadata))
                committed = true
            }
            try Task.checkCancellation()
            return imported
        } catch {
            for file in imported { try? store.remove(for: file.sourceId) }
            throw error
        }
    }

    func read(sourceId: String, offset: UInt64, length: UInt32) throws -> Data {
        try Task.checkCancellation()
        guard let store = sources else { throw ChatFileFailure.unavailable }
        let url = try sourceURL(sourceId, store: store)
        let descriptor = try openRegular(url, flags: O_RDONLY)
        defer { Darwin.close(descriptor) }
        let size = try size(of: descriptor)
        guard length <= 2_000_000, offset <= size, UInt64(length) <= size - offset else {
            throw ChatFileFailure.invalidRange
        }
        var result = Data(count: Int(length))
        try result.withUnsafeMutableBytes { bytes in
            var done = 0
            while done < bytes.count {
                let count = Darwin.pread(
                    descriptor,
                    bytes.baseAddress!.advanced(by: done),
                    bytes.count - done,
                    off_t(offset) + off_t(done)
                )
                if count < 0, errno == EINTR { continue }
                guard count > 0 else { throw ChatFileFailure.inputOutput }
                done += count
            }
        }
        return result
    }

    func release(sourceId: String) throws {
        let store = try prepare(sources)
        let url = try sourceURL(sourceId, store: store)
        try removeIfPresent(url)
        try syncDirectory(url.deletingLastPathComponent())
    }

    func beginExport(size: UInt32) throws -> String {
        try Task.checkCancellation()
        let store = try prepare(exports)
        let id = UUID().uuidString
        let url = store.fileURL(for: id + ".bin")
        let descriptor = url.path.withCString {
            Darwin.open($0, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW, mode_t(0o600))
        }
        guard descriptor >= 0 else { throw ChatFileFailure.inputOutput }
        Darwin.close(descriptor)
        do {
            try FileManager.default.setAttributes(
                [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
                ofItemAtPath: url.path
            )
        } catch {
            try? FileManager.default.removeItem(at: url)
            throw error
        }
        pendingExports[id] = Export(size: UInt64(size))
        return id
    }

    func write(exportId: String, offset: UInt64, data: Data) throws {
        try Task.checkCancellation()
        guard let store = exports else { throw ChatFileFailure.unavailable }
        try validate(exportId)
        guard var state = pendingExports[exportId], !state.presenting,
              offset == state.written, data.count <= 2_000_000,
              UInt64(data.count) <= state.size - state.written else {
            throw ChatFileFailure.invalidRange
        }
        let url = store.fileURL(for: exportId + ".bin")
        let descriptor = try openRegular(url, flags: O_WRONLY)
        defer { Darwin.close(descriptor) }
        guard try size(of: descriptor) == state.written else { throw ChatFileFailure.invalidRange }
        try data.withUnsafeBytes { bytes in
            var done = 0
            while done < bytes.count {
                let count = Darwin.pwrite(
                    descriptor,
                    bytes.baseAddress!.advanced(by: done),
                    bytes.count - done,
                    off_t(offset) + off_t(done)
                )
                if count < 0, errno == EINTR { continue }
                guard count > 0 else { throw ChatFileFailure.inputOutput }
                done += count
            }
        }
        state.written += UInt64(data.count)
        pendingExports[exportId] = state
    }

    /// Seal before presenting Files. The UI only receives an exact-size file,
    /// and later writes/cancellation cannot mutate it while Files copies it.
    func sealExport(exportId: String) async throws -> URL {
        try Task.checkCancellation()
        guard let store = exports else { throw ChatFileFailure.unavailable }
        try validate(exportId)
        guard var state = pendingExports[exportId], !state.presenting, state.written == state.size else {
            throw ChatFileFailure.invalidRange
        }
        let url = store.fileURL(for: exportId + ".bin")
        guard UInt64(try fileSize(url)) == state.size else { throw ChatFileFailure.invalidRange }
        try protectAndSync(url)
        state.presenting = true
        pendingExports[exportId] = state
        // Derive a safe extension from the sealed bytes, never a remote MIME
        // claim. The save UI still never opens or plays the document.
        let detected = await metadata(for: url, size: UInt32(state.size))
        try Task.checkCancellation()
        guard let suffix = Self.mediaExtensions[detected.mimeType] else { return url }
        let destination = store.fileURL(for: exportId + "." + suffix)
        try FileManager.default.moveItem(at: url, to: destination)
        return destination
    }

    /// Only the private staging file is removed, never the user's chosen copy.
    func completeExport(exportId: String) throws {
        try validate(exportId)
        let store = try prepare(exports)
        try removeIfPresent(store.fileURL(for: exportId + ".bin"))
        for suffix in Self.mediaExtensions.values {
            try removeIfPresent(store.fileURL(for: exportId + "." + suffix))
        }
        pendingExports.removeValue(forKey: exportId)
    }

    func cancelExport(exportId: String) throws {
        try validate(exportId)
        // finish owns cleanup while a save sheet is in flight. Its cancellation
        // handler dismisses the sheet before releasing the staging file.
        guard pendingExports[exportId]?.presenting != true else { return }
        try completeExport(exportId: exportId)
    }
}

private extension TrUAPIChatFileStore {
    private func prepare(_ store: AttachmentStoring?) throws -> AttachmentStoring {
        guard let store else { throw ChatFileFailure.unavailable }
        try store.createDirectoryIfNeeded()
        let directory = store.fileURL(for: "").standardizedFileURL
        try FileManager.default.setAttributes([
            .posixPermissions: 0o700,
            .protectionKey: FileProtectionType.completeUntilFirstUserAuthentication
        ], ofItemAtPath: directory.path)
        try syncDirectory(directory)
        try syncDirectory(directory.deletingLastPathComponent())
        return store
    }

    private func validate(_ id: String) throws {
        guard let uuid = UUID(uuidString: id), uuid.uuidString == id else { throw ChatFileFailure.invalidHandle }
    }

    private func sourceURL(_ id: String, store: AttachmentStoring) throws -> URL {
        try validate(id)
        return store.fileURL(for: id)
    }

    private func openRegular(_ url: URL, flags: Int32) throws -> Int32 {
        let descriptor = url.path.withCString { Darwin.open($0, flags | O_NOFOLLOW | O_NONBLOCK) }
        guard descriptor >= 0 else { throw ChatFileFailure.inputOutput }
        do {
            _ = try size(of: descriptor)
            return descriptor
        } catch {
            Darwin.close(descriptor)
            throw error
        }
    }

    private func size(of descriptor: Int32) throws -> UInt64 {
        var info = stat()
        guard Darwin.fstat(descriptor, &info) == 0,
              (info.st_mode & S_IFMT) == S_IFREG,
              info.st_size >= 0, UInt64(info.st_size) <= UInt64(UInt32.max) else {
            throw ChatFileFailure.invalidSize
        }
        return UInt64(info.st_size)
    }

    private func fileSize(_ url: URL) throws -> UInt32 {
        let descriptor = try openRegular(url, flags: O_RDONLY)
        defer { Darwin.close(descriptor) }
        return UInt32(try size(of: descriptor))
    }

    private func protectAndSync(_ url: URL) throws {
        try FileManager.default.setAttributes(
            [.protectionKey: FileProtectionType.completeUntilFirstUserAuthentication],
            ofItemAtPath: url.path
        )
        let descriptor = try openRegular(url, flags: O_WRONLY)
        defer { Darwin.close(descriptor) }
        guard Darwin.fsync(descriptor) == 0, Darwin.fcntl(descriptor, F_FULLFSYNC) == 0 else {
            throw ChatFileFailure.inputOutput
        }
    }

    private func syncDirectory(_ url: URL) throws {
        let descriptor = url.path.withCString { Darwin.open($0, O_RDONLY | O_DIRECTORY | O_NOFOLLOW) }
        guard descriptor >= 0 else { throw ChatFileFailure.inputOutput }
        defer { Darwin.close(descriptor) }
        guard Darwin.fsync(descriptor) == 0 else { throw ChatFileFailure.inputOutput }
    }

    private func removeIfPresent(_ url: URL) throws {
        let result = url.path.withCString { Darwin.unlink($0) }
        guard result == 0 || errno == ENOENT else { throw ChatFileFailure.inputOutput }
    }

    private func metadata(for url: URL, size: UInt32) async -> HostNativeChatAttachmentMetadata {
        // Detect raster image content from the immutable bytes, not a filename
        // or provider MIME claim. SVG/HTML and unrecognized files remain opaque.
        guard let source = CGImageSourceCreateWithURL(url as CFURL, nil),
              let identifier = CGImageSourceGetType(source),
              let type = UTType(identifier as String), type.conforms(to: .image), !type.conforms(to: .svg),
              let properties = CGImageSourceCopyPropertiesAtIndex(source, 0, nil) as? [CFString: Any],
              let width = properties[kCGImagePropertyPixelWidth] as? NSNumber,
              let height = properties[kCGImagePropertyPixelHeight] as? NSNumber,
              width.doubleValue > 0, width.doubleValue <= Double(UInt32.max),
              height.doubleValue > 0, height.doubleValue <= Double(UInt32.max) else {
            return await videoMetadata(for: url, size: size)
                ?? HostNativeChatAttachmentMetadata(mimeType: "application/octet-stream", sizeBytes: size, kind: .file)
        }
        // Reuse the native Chat's bounded image downsampling and UTF-8 BlurHash
        // convention, but do not transcode or consult the mutable original.
        let thumbnail = UIImage.downsampleImage(
            at: url,
            maxSideSize: BlurHashConfiguration.encodingMaximumSide,
            scale: 1
        )?
            .blurHash(numberOfComponents: BlurHashConfiguration.components)
            .flatMap { BlurHash($0) }?
            .toData()
        return HostNativeChatAttachmentMetadata(
            mimeType: type.preferredMIMEType ?? "application/octet-stream",
            sizeBytes: size,
            kind: .image(width: width.uint32Value, height: height.uint32Value, thumbnail: thumbnail)
        )
    }

    private func videoMetadata(for url: URL, size: UInt32) async -> HostNativeChatAttachmentMetadata? {
        // Only known self-contained ISO-BMFF/QuickTime containers are probed.
        // No playlists, URLs, filename-based MIME guesses or external references.
        guard let handle = try? FileHandle(forReadingFrom: url) else { return nil }
        let header = try? handle.read(upToCount: 32)
        try? handle.close()
        guard let header, header.count >= 12,
              header[4..<8].elementsEqual([0x66, 0x74, 0x79, 0x70]) else { return nil }
        let mime: String
        guard let brand = String(bytes: header[8..<12], encoding: .utf8) else { return nil }
        switch brand {
        case "qt  ": mime = "video/quicktime"
        case "isom", "iso2", "iso4", "iso5", "iso6", "mp41", "mp42", "avc1", "M4V ", "M4VH", "M4VP":
            mime = "video/mp4"
        default: return nil
        }
        let asset = AVURLAsset(url: url, options: [
            AVURLAssetReferenceRestrictionsKey: AVAssetReferenceRestrictions.forbidAll.rawValue
        ])
        let generator = AVAssetImageGenerator(asset: asset)
        generator.appliesPreferredTrackTransform = true
        generator.maximumSize = BlurHashConfiguration.encodingPreviewSize
        return try? await withThrowingTaskGroup(of: HostNativeChatAttachmentMetadata.self) { group in
            group.addTask {
                guard !(try await asset.loadTracks(withMediaType: .video)).isEmpty else {
                    throw ChatFileFailure.unavailable
                }
                let duration = try await asset.load(.duration)
                guard duration.seconds.isFinite, duration.seconds >= 0,
                      duration.seconds <= Double(UInt32.max) else {
                    throw ChatFileFailure.invalidSize
                }
                var thumbnail: Data?
                // Same bounded frame/BlurHash convention as PHVideoAttachmentProvider.
                if let image = try? await generator.image(at: .zero).image {
                    thumbnail = UIImage(cgImage: image)
                        .blurHash(numberOfComponents: BlurHashConfiguration.components)
                        .flatMap { BlurHash($0) }?
                        .toData()
                }
                try Task.checkCancellation()
                return HostNativeChatAttachmentMetadata(
                    mimeType: mime,
                    sizeBytes: size,
                    kind: .video(
                        durationSeconds: UInt32(duration.seconds),
                        thumbnail: thumbnail
                    )
                )
            }
            group.addTask {
                try await Task.sleep(for: .seconds(10))
                asset.cancelLoading()
                generator.cancelAllCGImageGeneration()
                throw ChatFileFailure.unavailable
            }
            defer {
                group.cancelAll()
                asset.cancelLoading()
                generator.cancelAllCGImageGeneration()
            }
            guard let result = try await group.next() else { throw ChatFileFailure.unavailable }
            return result
        }
    }
}

enum ChatFileFailure: Error {
    case unavailable
    case invalidHandle
    case invalidRange
    case invalidSize
    case inputOutput
    case userCancelled
}
