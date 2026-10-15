import CryptoKit
import Foundation
import ImageIO
import UniformTypeIdentifiers

/// A media item of a product body that the user opens (02-DESIGN §6.2, PARITY row 4).
struct ProductMediaOpenRequest: Equatable {
    /// The source that the body keeps: an inline `data:` URL, a preimage source or an `https` URL.
    let source: String
    let info: Chat.LocalMessage.Content.ProductMediaInfo
}

/// Reads the bytes of a media item through the resource resolver, and writes them to a local file that the system
/// viewers open.
struct ProductMediaFiles: Sendable {
    let resolver: ChatResourceResolving
    let directory: URL

    init(
        resolver: ChatResourceResolving,
        directory: URL = FileManager.default.temporaryDirectory.appending(
            path: "ProductMedia",
            directoryHint: .isDirectory
        )
    ) {
        self.resolver = resolver
        self.directory = directory
    }

    /// The local file of the item, or nil when the bytes do not load.
    func file(for request: ProductMediaOpenRequest) async -> URL? {
        guard let resource = ChatResource(source: request.source),
              let data = await resolver.data(for: resource),
              !data.isEmpty else {
            return nil
        }

        // One folder for each source, so two files with the same name do not replace each other.
        let digest = SHA256.hash(data: Data(request.source.utf8)).prefix(8).map { String(format: "%02x", $0) }.joined()
        let folder = directory.appending(path: digest, directoryHint: .isDirectory)
        let url = folder.appending(path: Self.fileName(of: request.info, data: data))

        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            try data.write(to: url, options: .atomic)
            return url
        } catch {
            return nil
        }
    }

    /// The name of the local file. A file keeps the name that the product gave, without a path. An image and a
    /// video get the extension of their type, so the viewers know how to read them.
    static func fileName(of info: Chat.LocalMessage.Content.ProductMediaInfo, data: Data) -> String {
        if let fileName = info.fileName {
            let name = fileName
                .replacingOccurrences(of: "/", with: "_")
                .replacingOccurrences(of: "\\", with: "_")
                .trimmingCharacters(in: .whitespacesAndNewlines)
            if !name.isEmpty, name != ".", name != ".." {
                return name
            }
        }

        let type = UTType(mimeType: info.mimeType) ?? imageType(of: data)
        let fallback =
            switch info.kind {
            case .image: "png"
            case .video: "mp4"
            case .file: "bin"
            }
        return "media." + (type?.preferredFilenameExtension ?? fallback)
    }

    private static func imageType(of data: Data) -> UTType? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let identifier = CGImageSourceGetType(source) as String? else {
            return nil
        }
        return UTType(identifier)
    }
}

extension ChatProductRoomPresenter {
    /// Opens a media item of a product body: an image full screen, a video in the player, and a file in the
    /// document preview, from which the user opens or shares it. Gives false when the bytes do not load. Then the
    /// bubble offers to try again.
    @discardableResult
    func openMedia(_ request: ProductMediaOpenRequest) async -> Bool {
        let kind = String(describing: request.info.kind)
        guard let url = await ProductMediaFiles(resolver: resolver).file(for: request) else {
            logger.warning("A \(kind) of room \(roomId) did not load")
            mark("media", "\(kind) failed")
            return false
        }

        wireframe.showProductMedia(url, kind: request.info.kind, from: view)
        mark("media", "\(kind) opened \(url.lastPathComponent)")
        return true
    }

    /// The request for the media item at `index` of a message, or nil when the message has no such item.
    func mediaRequest(of messageId: Chat.MessageId, at index: Int) -> ProductMediaOpenRequest? {
        guard case let .productBody(body) = messages[messageId]?.content else {
            return nil
        }

        switch body {
        case let .richText(richText) where richText.media.indices.contains(index):
            return ProductMediaOpenRequest(source: richText.media[index], info: richText.info(at: index))
        case let .file(file) where index == 0:
            return ProductMediaOpenRequest(
                source: file.url,
                info: .init(kind: .file, mimeType: file.mimeType, fileName: file.fileName, sizeBytes: file.sizeBytes)
            )
        case .richText,
             .file,
             .actions:
            return nil
        }
    }
}
