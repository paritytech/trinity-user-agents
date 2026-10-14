import Foundation
import SubstrateSdk
import SubstrateOperation

final class TaggedTransactionQueueApiMock: TaggedTransactionQueueApiProtocol {
    enum Response {
        case validity(TransactionValidity)
        case failure(Error)
    }

    private let mutex = NSLock()
    private var storedResponses: [Response] = []
    private var storedFallback: Response = .validity(.valid)
    private var storedCallCount = 0
    private var storedValidatedBodies: [Data] = []

    var responses: [Response] {
        get { mutex.withLock { storedResponses } }
        set { mutex.withLock { storedResponses = newValue } }
    }

    var fallback: Response {
        get { mutex.withLock { storedFallback } }
        set { mutex.withLock { storedFallback = newValue } }
    }

    var callCount: Int {
        mutex.withLock { storedCallCount }
    }

    var validatedBodies: [Data] {
        mutex.withLock { storedValidatedBodies }
    }

    func validateTransaction(
        chainId _: ChainId,
        source _: TransactionSource,
        extrinsic: Data,
        at _: BlockHashData
    ) async throws -> TransactionValidity {
        let response = nextResponse(for: extrinsic)

        switch response {
        case let .validity(validity):
            return validity
        case let .failure(error):
            throw error
        }
    }
}

private extension TaggedTransactionQueueApiMock {
    func nextResponse(for extrinsic: Data) -> Response {
        mutex.withLock {
            let response = storedCallCount < storedResponses.count
                ? storedResponses[storedCallCount]
                : storedFallback
            storedCallCount += 1
            storedValidatedBodies.append(extrinsic)
            return response
        }
    }
}
