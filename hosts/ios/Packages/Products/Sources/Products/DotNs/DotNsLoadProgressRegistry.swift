import AsyncExtensions
import Foundation
import os

protocol DotNsLoadProgressRegistryProtocol {
    func observe(domain: String) -> AnyAsyncSequence<DotNsLoadProgress>
    func markResolved(domain: String)
    func markDownload(domain: String, downloaded: Int64, total: Int64?)
    func markUnpacking(domain: String)
    func markCompleted(domain: String)
    func markFailed(domain: String, error: Error)
    func clear()
}

/// Single source of truth for per-domain `DotNsLoadProgress`. The resolver drives transitions as it
/// downloads and unpacks; `observe(domain:)` hands the host UI a hot latest-value stream.
final class DotNsLoadProgressRegistry: DotNsLoadProgressRegistryProtocol {
    private let subjects = OSAllocatedUnfairLock<[String: AsyncCurrentValueSubject<DotNsLoadProgress>]>(
        initialState: [:]
    )

    func observe(domain: String) -> AnyAsyncSequence<DotNsLoadProgress> {
        subjects.withLock { subjects in
            subject(in: &subjects, domain: domain).eraseToAnyAsyncSequence()
        }
    }

    func markResolved(domain: String) {
        send(.resolved, to: domain)
    }

    func markDownload(domain: String, downloaded: Int64, total: Int64?) {
        let fraction = total.map { total -> Double in
            guard total > 0 else { return 0 }
            return min(max(Double(downloaded) / Double(total), 0), 1)
        }
        send(.downloading(fraction: fraction ?? 0), to: domain)
    }

    func markUnpacking(domain: String) {
        send(.unpacking, to: domain)
    }

    func markCompleted(domain: String) {
        send(.completed, to: domain)
    }

    func markFailed(domain: String, error: Error) {
        send(.failed(error), to: domain)
    }

    func clear() {
        subjects.withLock { $0.removeAll() }
    }

    private func send(_ progress: DotNsLoadProgress, to domain: String) {
        subjects.withLock { subjects in
            subject(in: &subjects, domain: domain).send(progress)
        }
    }

    private func subject(
        in subjects: inout [String: AsyncCurrentValueSubject<DotNsLoadProgress>],
        domain: String
    ) -> AsyncCurrentValueSubject<DotNsLoadProgress> {
        if let existing = subjects[domain] {
            return existing
        }
        let created = AsyncCurrentValueSubject<DotNsLoadProgress>(.idle)
        subjects[domain] = created
        return created
    }
}
