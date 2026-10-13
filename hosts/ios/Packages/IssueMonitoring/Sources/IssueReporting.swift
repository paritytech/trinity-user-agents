import Foundation

/// Sends critical, flow-breaking issues. See `.claude/docs/code/error-handling.md`.
public protocol IssueReporting: Sendable {
    func report(_ issue: CriticalIssue)
}

/// An error that names its case for reporting without exposing associated values.
public protocol ReportableError: Error {
    var reportCode: String { get }
}

public struct CriticalIssue: Sendable {
    public let flow: String
    public let kind: String
    public let errorType: String?
    public let errorCode: String?
    public let counters: [String: Int]

    public init(flow: StaticString, kind: StaticString, error: Error? = nil, counters: [String: Int] = [:]) {
        self.flow = "\(flow)"
        self.kind = "\(kind)"
        errorType = error.map { String(reflecting: type(of: $0)) }
        errorCode = (error as? ReportableError)?.reportCode
        self.counters = counters
    }
}

public struct NoopIssueReporter: IssueReporting {
    public init() {}

    public func report(_: CriticalIssue) {}
}
