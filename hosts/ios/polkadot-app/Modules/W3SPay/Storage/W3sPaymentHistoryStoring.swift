import AsyncExtensions
import Foundation

protocol W3sPaymentHistoryStoring: Sendable {
    func save(_ record: W3sPaymentRecord) async throws
    func updateStatus(
        paymentId: String,
        status: W3sPaymentRecord.Status
    ) async throws
    func fetch(byId id: String) async throws -> W3sPaymentRecord?
    func observeAll() -> AnyAsyncSequence<[W3sPaymentRecord]>
    func observeRecord(paymentId: String) -> AnyAsyncSequence<W3sPaymentRecord?>
}
