import Foundation
import AsyncExtensions

// MARK: - Locale

extension ProductsNativeApi {
    func subscribeLocale() -> AnyAsyncSequence<String> {
        localeProvider.subscribe()
    }
}
