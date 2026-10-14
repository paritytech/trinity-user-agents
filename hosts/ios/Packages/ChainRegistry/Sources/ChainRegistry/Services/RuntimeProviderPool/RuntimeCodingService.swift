import Foundation
import Operation_iOS
import SubstrateSdk

public extension RuntimeCodingServiceProtocol {
    func fetchCoderFactory(
        runningIn manager: OperationManagerProtocol,
        completion successClosure: @escaping (RuntimeCoderFactoryProtocol) -> Void,
        errorClosure: @escaping (Error) -> Void
    ) {
        let operation = fetchCoderFactoryOperation()

        operation.completionBlock = {
            DispatchQueue.main.async {
                do {
                    let factory = try operation.extractNoCancellableResultData()
                    successClosure(factory)
                } catch {
                    errorClosure(error)
                }
            }
        }

        manager.enqueue(operations: [operation], in: .transient)
    }
}
