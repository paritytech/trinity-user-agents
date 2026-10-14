import ExtrinsicService
import Foundation
import Operation_iOS
import SubstrateSdk
@testable import Coinage

/// Never submits: the external payment pipeline hands submission to the durability service, so the
/// monitor is only a construction dependency here.
struct StubExtrinsicMonitorFactory: ExtrinsicSubmitMonitorFactoryProtocol {
    struct NotSupported: Error {}

    func submitAndMonitorWrapper(
        extrinsicBuilderClosure _: @escaping ExtrinsicBuilderClosure,
        origin _: ExtrinsicOriginDefining,
        params _: ExtrinsicSubmissionParams
    ) -> CompoundOperationWrapper<ExtrinsicMonitorSubmission> {
        .createWithError(NotSupported())
    }

    func submitAndMonitorWrapper(
        extrinsicBuilderClosure _: @escaping ExtrinsicBuilderIndexedClosure,
        origin _: ExtrinsicOriginDefining,
        indexes _: IndexSet,
        params _: ExtrinsicIndexedSubmissionParams
    ) -> CompoundOperationWrapper<ExtrinsicRetriableResult<ExtrinsicMonitorSubmission>> {
        .createWithError(NotSupported())
    }
}
