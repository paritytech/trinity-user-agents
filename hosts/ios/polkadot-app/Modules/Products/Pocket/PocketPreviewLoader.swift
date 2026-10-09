import Foundation
import Products
import TrUAPIHost

/// Reads a file out of a product executable's content archive, fetching the
/// archive first when the host does not already hold it.
protocol PocketArchiveReading {
    /// Reads at most `maxBytes`, refusing anything longer rather than holding
    /// it: what the file weighs is the product's choice, and this is read
    /// before the user has approved anything.
    func file(contentId: ProductId, path: String, maxBytes: Int) async throws -> Data
}

enum PocketPreviewError: Error, CustomStringConvertible {
    case notReachable(String)
    case tooLarge(bytes: Int)
    case notReadable

    var description: String {
        switch self {
        case let .notReachable(url): "'\(url)' is not an address the preview can be read from"
        case let .tooLarge(bytes): "preview is \(bytes) bytes, larger than \(PocketPreviewLoader.maxBytes)"
        case .notReadable: "preview is not readable text"
        }
    }
}

/// Reads a published card's static face, the one the approval sheet shows.
///
/// This runs before the user has approved anything, so how much there is to
/// read is the product's choice. The bound goes to whoever does the reading
/// rather than being applied to what came back: a face refused once it is
/// already held whole is a face the product chose the size of.
struct PocketPreviewLoader {
    static let maxBytes = 256 * 1_024

    private let archive: any PocketArchiveReading
    private let fetch: @Sendable (URL, Int) async throws -> Data

    init(archive: any PocketArchiveReading, fetch: @escaping @Sendable (URL, Int) async throws -> Data) {
        self.archive = archive
        self.fetch = fetch
    }

    func load(contentId: ProductId, preview: PocketCardPreview) async throws -> RendererNode {
        let data =
            switch preview {
            case let .archive(path):
                try await archive.file(contentId: contentId, path: path, maxBytes: Self.maxBytes)
            case let .url(url):
                try await fetched(url)
            }

        // A reader that handed back more than it was asked for is still refused.
        guard data.count <= Self.maxBytes else { throw PocketPreviewError.tooLarge(bytes: data.count) }
        guard let json = String(data: data, encoding: .utf8) else { throw PocketPreviewError.notReadable }

        return try parseRendererNodeJson(json: json)
    }

    private func fetched(_ url: String) async throws -> Data {
        guard let url = URL(string: url) else { throw PocketPreviewError.notReachable(url) }

        return try await fetch(url, Self.maxBytes)
    }
}

/// Reads a face over the network without letting the other end decide how much
/// the host holds: the body is taken as it arrives and dropped as soon as it
/// passes the bound, rather than buffered whole and measured afterwards.
enum PocketPreviewFetch {
    @Sendable
    static func bounded(_ url: URL, maxBytes: Int) async throws -> Data {
        let (bytes, _) = try await URLSession.shared.bytes(from: url)

        var data = Data()
        for try await byte in bytes {
            data.append(byte)
            guard data.count <= maxBytes else { throw PocketPreviewError.tooLarge(bytes: data.count) }
        }

        return data
    }
}
