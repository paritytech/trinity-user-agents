import Foundation
import TrUAPIHost
@testable import polkadot_app

/// A materials resolution a test feeds by hand, which records what the app reports to it.
final class StubMaterialsResolution: MaterialsResolving, @unchecked Sendable {
    struct Report: Equatable {
        let node: UInt32
        let material: Data?
        let reason: MaterialFailure
    }

    let resolved: AsyncStream<ResolvedMaterials>
    private let continuation: AsyncStream<ResolvedMaterials>.Continuation
    private let onCancel: () -> Void
    private(set) var reports: [Report] = []

    init(onCancel: @escaping () -> Void = {}) {
        (resolved, continuation) = AsyncStream.makeStream(of: ResolvedMaterials.self)
        self.onCancel = onCancel
    }

    func send(_ update: ResolvedMaterials) {
        continuation.yield(update)
    }

    func report(node: UInt32, material: Bytes32?, reason: MaterialFailure) {
        reports.append(Report(node: node, material: material, reason: reason))
    }

    func cancel() {
        onCancel()
        continuation.finish()
    }
}
