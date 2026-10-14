import Foundation
import AsyncExtensions

protocol LocaleProviding: Sendable {
    /// Emits the user's current UI language as a BCP 47 tag.
    func subscribe() -> AnyAsyncSequence<String>
}

/// Reports the user's preferred language from the system. iOS relaunches the app
/// when the user changes its language in Settings, so the tag is fixed for the
/// process lifetime and emitted once.
final class SystemLocaleProvider: LocaleProviding {
    static let shared = SystemLocaleProvider()

    func subscribe() -> AnyAsyncSequence<String> {
        let (stream, continuation) = AsyncStream<String>.makeStream(bufferingPolicy: .bufferingNewest(1))
        continuation.yield(Self.currentLanguageTag())
        return stream.eraseToAnyAsyncSequence()
    }

    static func currentLanguageTag() -> String {
        if let preferred = Locale.preferredLanguages.first, !preferred.isEmpty {
            return preferred
        }

        return Locale.current.identifier(.bcp47)
    }
}
