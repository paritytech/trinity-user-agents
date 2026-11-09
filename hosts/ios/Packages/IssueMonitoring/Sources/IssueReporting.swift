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

extension CriticalIssue {
    /// The event this issue leaves as. Its grouping key is flow and kind rather
    /// than the name they spell, because that name is derived from both, and its
    /// severity is `error` because a flow broke rather than progressed.
    func hostEvent(origin: HostEventOrigin, monotonicMilliseconds: Int) -> HostEvent {
        var tags = ["flow": flow, "kind": kind, "host.os_version": origin.osVersion]

        if let errorType {
            tags["errorType"] = errorType
        }

        if let errorCode {
            tags["errorCode"] = errorCode
        }

        return HostEvent.make(
            name: "\(flow).\(kind)",
            origin: origin,
            monotonicMilliseconds: monotonicMilliseconds,
            level: .error,
            fingerprint: [flow, kind],
            tags: tags,
            extras: ["app.launch_id": .string(origin.launchID.uuidString.lowercased())],
            counters: counters
        )
    }
}

/// Reports an issue as a host event, so the issues and the lifecycle events a
/// host sends leave through one sender.
struct HostEventIssueReporter: IssueReporting {
    private let recorder: HostEventRecording
    private let origin: HostEventOrigin
    private let monotonicMilliseconds: @Sendable () -> Int

    init(
        recorder: HostEventRecording,
        origin: HostEventOrigin,
        monotonicMilliseconds: @escaping @Sendable () -> Int
    ) {
        self.recorder = recorder
        self.origin = origin
        self.monotonicMilliseconds = monotonicMilliseconds
    }

    func report(_ issue: CriticalIssue) {
        recorder.record(issue.hostEvent(origin: origin, monotonicMilliseconds: monotonicMilliseconds()))
    }
}

public struct NoopIssueReporter: IssueReporting {
    public init() {}

    public func report(_: CriticalIssue) {}
}
