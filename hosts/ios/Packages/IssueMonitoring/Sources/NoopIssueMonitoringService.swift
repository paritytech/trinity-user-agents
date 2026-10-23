import Foundation

public final class NoopIssueMonitoringService: IssueMonitoringServiceProtocol {
    public init() {}

    public func setup() {}

    public func recordProcessStarted(_: HostProcessStartedContext) {}
}
