import Foundation
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

    private let fetch: @Sendable (URL, Int) async throws -> Data
    private let resolver: any WidgetDesignTokenResolving

    init(
        fetch: @escaping @Sendable (URL, Int) async throws -> Data = PocketPreviewFetch.bounded,
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

/// The one refusal this screen owns. Everything else it shows comes from
/// the core reading the face.
enum DebugPocketFacePreviewError: Error, CustomStringConvertible {
    case notUtf8

    var description: String { "the file is not UTF-8" }
}
