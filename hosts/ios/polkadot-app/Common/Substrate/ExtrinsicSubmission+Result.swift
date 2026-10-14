import Foundation
import ExtrinsicService

extension ExtrinsicMonitorSubmission {
    func ensureSuccess() throws {
        switch status {
        case .success:
            return
        case let .failure(failedExtrinsic):
            throw failedExtrinsic.error
        }
    }
}
