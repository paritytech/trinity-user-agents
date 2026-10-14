import Foundation

/// Builds a URL from a navigation destination that may omit the scheme
/// (e.g. `getcash.paseo/#/`). Foundation treats a scheme-less string as a
/// relative path, so a placeholder scheme is prepended to get the host parsed.
enum NavigationDestinationURL {
    static let placeholderScheme = "https"

    static func make(_ destination: String) -> URL? {
        guard let url = URL(string: destination) else {
            return nil
        }

        guard url.scheme == nil else {
            return url
        }

        return URL(string: placeholderScheme + "://" + destination)
    }
}
