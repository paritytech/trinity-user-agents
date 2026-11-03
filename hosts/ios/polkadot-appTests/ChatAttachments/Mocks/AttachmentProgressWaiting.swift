import StructuredConcurrency
import StructuredConcurrencyTestSupport

@testable import polkadot_app

extension AttachmentLoadProgressProvidable {
    /// Returns once the attachment reports any progress, failing rather than hanging if it never does.
    func waitForFirstEvent(of attachmentId: AttachmentId) async throws {
        let states = await subscribeState(for: attachmentId)
        try await withTimeout(.hangGuard) {
            for try await state in states where state != nil {
                return
            }
            throw CancellationError()
        }
    }
}
