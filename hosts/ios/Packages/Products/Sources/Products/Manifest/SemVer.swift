import Foundation

/// `major.minor.patch` with the manifest specification's optional build identifier.
public struct SemVer: Hashable, Sendable {
    public let major: Int
    public let minor: Int
    public let patch: Int
    public let build: String?

    /// Products that publish no manifest have no version to report.
    public static let zero = SemVer(major: 0, minor: 0, patch: 0, build: nil)

    public init(major: Int, minor: Int, patch: Int, build: String?) {
        self.major = major
        self.minor = minor
        self.patch = patch
        self.build = build
    }
}
