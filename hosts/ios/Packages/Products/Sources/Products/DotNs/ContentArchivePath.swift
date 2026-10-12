import Foundation

/// Resolves a path written by a product against the archive it belongs to,
/// answering nil for one that would leave it.
///
/// `..` and absolute paths are refused before the filesystem is touched, so a
/// link the archive itself carries is never walked into place. Both sides are
/// then symlink-resolved, because the archive's contents are the product's word
/// as much as the path is, and because on iOS the container is reached through
/// the `/var` -> `/private/var` link, which a raw comparison would never match.
public enum ContentArchivePath {
    public static func inside(_ root: URL, path: String) -> URL? {
        let components = path.split(separator: "/", omittingEmptySubsequences: true)
        guard !path.hasPrefix("/"), !components.contains("..") else { return nil }

        let base = root.resolvingSymlinksInPath().standardizedFileURL
        let file = root.appendingPathComponent(path).resolvingSymlinksInPath().standardizedFileURL

        // The separator is what makes this a question about directories: a
        // sibling named `<root>.staging` starts with the root's own path.
        guard file.path == base.path || file.path.hasPrefix(base.path + "/") else { return nil }

        return file
    }
}
