import BulletinChain
import Foundation
import Kingfisher
import Observation
import PolkadotUI
import Products
import TrUAPIHost

/// Fetches one face file and draws it, with nothing else in the way.
///
/// This is the loop for working on a face: no worker, no manifest, no product.
/// A face that is refused shows the decoder's own message, because that message
/// is the one that says what is wrong with the file.
@Observable
@MainActor
final class DebugPocketFacePreviewViewModel {
    var url = "http://127.0.0.1:5173/pocket/devicehood.json"
    private(set) var face: CustomMessageWidgetNode?
    private(set) var refusal: String?
    private(set) var isLoading = false
    // Counts draws, so drawing an unchanged face again still reloads its images.
    private(set) var draws = 0
    private var drawnURL: URL?

    /// Resolves images against the face that was drawn, not the address being typed.
    var imageResolver: WidgetImageResolver? {
        drawnURL.map(DebugPocketFacePreviewImages.resolver(faceURL:))
    }

    private let fetch: @Sendable (URL, Int) async throws -> Data
    private let resolver: any WidgetDesignTokenResolving

    init(
        // The face is edited between draws, so the app's HTTP cache must not answer for the server.
        fetch: @escaping @Sendable (URL, Int) async throws -> Data = { url, maxBytes in
            try await PocketPreviewFetch.read(
                URLRequest(url: url, cachePolicy: .reloadIgnoringLocalCacheData),
                maxBytes: maxBytes
            )
        },
        resolver: any WidgetDesignTokenResolving = WidgetDesignTokenResolver()
    ) {
        self.fetch = fetch
        self.resolver = resolver
    }

    func load() async {
        guard let address = URL(string: url) else {
            show(refusal: "'\(url)' is not an address")
            return
        }

        isLoading = true
        defer { isLoading = false }

        do {
            let data = try await fetch(address, PocketPreviewLoader.maxBytes)
            guard let json = String(bytes: data, encoding: .utf8) else {
                throw DebugPocketFacePreviewError.notUtf8
            }
            face = try parseRendererNodeJson(json: json).toWidgetNode(resolver: resolver)
            drawnURL = address
            draws += 1
            refusal = nil
        } catch {
            show(refusal: "\(error)")
        }
    }

    private func show(refusal message: String) {
        refusal = message
        face = nil
    }
}

/// Where a previewed face's images come from. The developer serves the folder that becomes the worker archive, so an
/// archive image is read from the face's server.
enum DebugPocketFacePreviewImages {
    static func resolver(faceURL: URL) -> WidgetImageResolver {
        let gateway = HexToCIDConverter()

        return .cached(
            // Drawing again is how an edited image is picked up, so no cache answers for the dev server. A CID names
            // content that never changes, so Bulletin images keep the cache.
            options: { source in
                guard case .archive = source else { return [] }
                return [.forceRefresh]
            },
            locate: { source in url(for: source, faceURL: faceURL, gateway: { gateway.ipfsURL(cid: $0) }) }
        )
    }

    /// Reads an archive path as a card does: a file name under the archive root, where a path that climbs above the
    /// root is refused and every other segment is literal. Resolving it as a URL instead would let `..`, a full
    /// address or `?` reach files and servers a card never could.
    static func url(
        for source: CustomMessageWidgetNode.ImageSource,
        faceURL: URL,
        gateway: (String) -> URL?
    ) -> URL? {
        switch source {
        case let .bulletin(cid):
            return gateway(cid)
        case let .archive(path):
            var segments: [String] = []
            for segment in path.split(separator: "/", omittingEmptySubsequences: false) {
                switch segment {
                case "",
                     ".":
                    continue
                case "..":
                    guard segments.popLast() != nil else { return nil }
                default:
                    segments.append(String(segment))
                }
            }

            var components = URLComponents()
            components.scheme = faceURL.scheme
            components.host = faceURL.host()
            components.port = faceURL.port
            components.percentEncodedPath = "/" + segments.map(encoded).joined(separator: "/")
            return components.url
        }
    }

    private static func encoded(_ segment: String) -> String {
        let literal = CharacterSet.urlPathAllowed.subtracting(CharacterSet(charactersIn: "/"))
        return segment.addingPercentEncoding(withAllowedCharacters: literal) ?? segment
    }
}

/// The one refusal this screen owns. Everything else it shows comes from
/// the core reading the face.
enum DebugPocketFacePreviewError: Error, CustomStringConvertible {
    case notUtf8

    var description: String { "the file is not UTF-8" }
}
