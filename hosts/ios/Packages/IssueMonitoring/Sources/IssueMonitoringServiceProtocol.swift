import Foundation

/// Crash and error reporting for non-production builds. The shipped Release build
/// resolves this to a no-op and links no reporting SDK at all.
public protocol IssueMonitoringServiceProtocol {
    func setup()
}
