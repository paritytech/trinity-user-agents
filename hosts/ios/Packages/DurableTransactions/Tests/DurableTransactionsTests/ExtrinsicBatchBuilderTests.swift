@testable import DurableTransactions
import ExtrinsicService
import Foundation
import Operation_iOS
import SubstrateSdk
import Testing

@Suite("Extrinsic batch grouping")
struct ExtrinsicBatchBuilderTests {
    /// A value-type origin: boxed anew every time it is used as an existential, so identity says nothing.
    private struct ValueOrigin: ExtrinsicOriginDefining {
        struct Unresolved: Error {}

        func createOriginResolutionWrapper(
            for _: @escaping () throws -> ExtrinsicOriginDefinitionDependency,
            extrinsicVersion _: Extrinsic.Version,
            purpose _: ExtrinsicOriginPurpose
        ) -> CompoundOperationWrapper<ExtrinsicOriginDefinitionResponse> {
            .createWithError(Unresolved())
        }
    }

    private func request(batchKey: String? = nil) -> DurableTxRequest {
        DurableTxRequest(builder: { $0 }, origin: ValueOrigin(), batchKey: batchKey.map(DurableTxBatchKey.init))
    }

    @Test("Requests sharing a batch key are built together, in request order")
    func sharedKeyGroups() {
        let groups = ExtrinsicBatchBuilder.groupByBatchKey([
            request(batchKey: "split"),
            request(batchKey: "other"),
            request(batchKey: "split")
        ])

        #expect(groups == [[0, 2], [1]])
    }

    @Test("Requests without a key are built on their own, whatever their origin's identity")
    func keylessRequestsStaySeparate() {
        let shared = ValueOrigin()
        let groups = ExtrinsicBatchBuilder.groupByBatchKey([
            DurableTxRequest(builder: { $0 }, origin: shared),
            DurableTxRequest(builder: { $0 }, origin: shared),
            request(batchKey: "split")
        ])

        #expect(groups == [[0], [1], [2]])
    }

    @Test("Group order follows the first request seen under each key")
    func firstSeenOrder() {
        let groups = ExtrinsicBatchBuilder.groupByBatchKey([
            request(),
            request(batchKey: "b"),
            request(batchKey: "a"),
            request(batchKey: "b")
        ])

        #expect(groups == [[0], [1, 3], [2]])
    }
}
