import BigInt
import Coinage
import Foundation

extension TrUAPINativeCoinage {
    /// Narrow service seam used by the regression harness; production always wraps the coordinator's service.
    struct Wallet: @unchecked Sendable {
        let denomination: () async throws -> DenominationBreakdownContext
        let preview: (BigUInt) async throws -> TransferPreview
        let prepare: (CoinSelectionResult, String, @escaping @Sendable () throws -> Void) async throws -> TransferMemo
        let retained: (String) async throws -> TransferMemo?
        /// Matches native subscribeStatuses: INPUT memo secrets; OUTPUT keyed by their derived public keys.
        let statuses: ([Data]) async throws -> [Data: CoinageTransferState]
        let accept: (BigUInt, [Data], String, String) async throws -> Void
        let incomingStatus: (String, String) async throws -> IncomingPaymentStatus

        init(service: any CoinageServicing) {
            denomination = { try await service.denominationContext() }
            preview = { try await service.previewTransfer(for: $0) }
            prepare = { selection, id, authorization in
                // The native execute overload retains custody atomically with tx registration.
                // Its memo cannot escape while merely provisional, even if Host transport has not accepted it.
                try await service.executeTransfer(
                    result: selection, groupId: id, custodyId: id, authorization: authorization
                ).memo
            }
            retained = { try await service.retainedTransfer(custodyId: $0) }
            statuses = { keys in
                for try await snapshot in service.transferStatusService.subscribeStatuses(coinKeys: keys) {
                    return snapshot
                }
                throw CancellationError()
            }
            accept = { amount, keys, id, product in
                try await service.incomingPaymentService.accept(
                    amount: amount, descriptor: .coins(secretKeys: keys), paymentId: id, productId: product
                )
            }
            incomingStatus = { id, product in
                let stream = try await service.incomingPaymentService.subscribeStatus(for: id, productId: product)
                for try await status in stream { return status }
                throw CancellationError()
            }
        }

        init(
            denomination: @escaping () async throws -> DenominationBreakdownContext,
            preview: @escaping (BigUInt) async throws -> TransferPreview,
            prepare: @escaping (CoinSelectionResult, String, @escaping @Sendable () throws -> Void) async throws -> TransferMemo,
            retained: @escaping (String) async throws -> TransferMemo?,
            statuses: @escaping ([Data]) async throws -> [Data: CoinageTransferState],
            accept: @escaping (BigUInt, [Data], String, String) async throws -> Void,
            incomingStatus: @escaping (String, String) async throws -> IncomingPaymentStatus
        ) {
            self.denomination = denomination
            self.preview = preview
            self.prepare = prepare
            self.retained = retained
            self.statuses = statuses
            self.accept = accept
            self.incomingStatus = incomingStatus
        }
    }
}
