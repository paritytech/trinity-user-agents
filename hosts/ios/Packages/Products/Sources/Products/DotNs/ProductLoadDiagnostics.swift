import Foundation
import FoundationExt
import IssueMonitoring

public struct ProductLoadDiagnostics: Sendable {
    public struct Thresholds: Sendable {
        public var failedAttempts: Int
        public var duration: TimeInterval

        public init(failedAttempts: Int = 5, duration: TimeInterval = 60) {
            self.failedAttempts = failedAttempts
            self.duration = duration
        }
    }

    let reporter: IssueReporting
    let dateProvider: DateProviding
    let thresholds: Thresholds

    public init(
        reporter: IssueReporting = NoopIssueReporter(),
        dateProvider: DateProviding = NowDateProvider(),
        thresholds: Thresholds = Thresholds()
    ) {
        self.reporter = reporter
        self.dateProvider = dateProvider
        self.thresholds = thresholds
    }
}

struct ProductLoadAttempts {
    let productId: ProductId
    private(set) var failed = 0
    private(set) var startedAt: Date
    private var stallReported = false

    init(productId: ProductId, startedAt: Date) {
        self.productId = productId
        self.startedAt = startedAt
    }

    mutating func recordFailure() {
        failed += 1
    }

    mutating func restart(at date: Date) {
        startedAt = date
    }

    mutating func claimStallReport(at date: Date, thresholds: ProductLoadDiagnostics.Thresholds) -> Bool {
        guard !stallReported,
              failed >= thresholds.failedAttempts || date.timeIntervalSince(startedAt) >= thresholds.duration else {
            return false
        }

        stallReported = true
        return true
    }
}

extension ProductLoadDiagnostics {
    func reportStall(_ attempts: ProductLoadAttempts, error: Error, at date: Date) {
        var counters = Self.counters(for: error)
        counters["attempts"] = attempts.failed
        counters["elapsedSeconds"] = Int(date.timeIntervalSince(attempts.startedAt))

        reporter.report(CriticalIssue(
            flow: "productLoad",
            kind: "contentFetchStalled",
            error: error,
            counters: counters
        ))
    }

    func reportUnavailable(_ error: Error) {
        guard !(error is DotNsContractError) else { return }

        reporter.report(
            CriticalIssue(
                flow: "productLoad",
                kind: "contentUnavailable",
                error: error,
                counters: Self.counters(for: error)
            )
        )
    }
}

private extension ProductLoadDiagnostics {
    static func counters(for error: Error) -> [String: Int] {
        switch error {
        case let urlError as URLError:
            ["urlErrorCode": urlError.errorCode]
        case let httpError as HTTPResponseError:
            ["httpStatus": httpError.statusCode]
        default:
            [:]
        }
    }
}
