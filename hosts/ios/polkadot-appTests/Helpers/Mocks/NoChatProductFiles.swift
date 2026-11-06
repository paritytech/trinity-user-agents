import Foundation
import Products
import TrUAPIHost

/// A product file provider with no files, for code that only needs one to exist.
struct NoChatProductFiles: ChatProductFileProviding {
    func load(for _: ProductId, relativePath _: String) -> Data? { nil }
    func manualScriptEntryPath(productId _: ProductId) -> String? { nil }
}
