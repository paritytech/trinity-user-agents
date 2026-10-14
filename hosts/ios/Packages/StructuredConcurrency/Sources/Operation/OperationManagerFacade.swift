import Foundation

enum OperationManagerFacade {
    static let sharedDefaultQueue: OperationQueue = {
        let queue = OperationQueue()
        return queue
    }()

    static let runtimeSyncQueue: OperationQueue = {
        let operationQueue = OperationQueue()
        operationQueue.qualityOfService = .userInitiated
        return operationQueue
    }()
}
