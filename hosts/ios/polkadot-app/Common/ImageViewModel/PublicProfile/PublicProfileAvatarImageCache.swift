import Foundation
import ImageIO
import Kingfisher
import TrUAPIHost
import UniformTypeIdentifiers

final class PublicProfileAvatarImageCache: Sendable {
    static let shared = PublicProfileAvatarImageCache()
    private let storage: DiskStorage.Backend<Data>

    init(name: String = "public-profile-avatars") {
        storage = ImageCache(name: name).diskStorage
        storage.config.expiration = .never
        storage.config.sizeLimit = 100 * 1_024 * 1_024
    }

    func data(for avatar: ProfileAvatar) throws -> Data? {
        guard let bytes = try storage.value(
            forKey: avatar.cid,
            extendingExpiration: .expirationTime(.never)
        ) else { return nil }
        let expectedType: UTType =
            switch avatar.format {
            case .png: .png
            case .jpeg: .jpeg
            case .webp: .webP
            case .gif: .gif
            }
        guard let source = CGImageSourceCreateWithData(bytes as CFData, nil),
              let type = CGImageSourceGetType(source), type as String == expectedType.identifier else {
            throw ImageError.Rejected(reason: "declared image format does not match cached bytes")
        }
        return bytes
    }

    func store(_ bytes: Data, for cid: String) throws {
        try storage.store(value: bytes, forKey: cid, writeOptions: .atomic)
        _ = try storage.removeSizeExceededValues()
    }

    func clear() throws {
        try storage.removeAll()
    }
}
