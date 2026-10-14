import Foundation
import Operation_iOS

open class BaseNetworkResultFactory<R>: NetworkResultFactoryProtocol {
    public typealias ResultType = R

    public init() {}

    public func createResult(
        data: Data?,
        response: URLResponse?,
        error: Error?
    ) -> Result<R, Error> {
        if let connectionError = error {
            return .failure(connectionError)
        }

        let statusCode = (response as? HTTPURLResponse)?.statusCode

        if [201, 202].contains(statusCode) {
            // 201, 202 are valid status codes
            // do nothing
        } else if let httpError = NetworkOperationHelper.createError(from: response) {
            return .failure(httpError)
        }

        guard let payload = data else {
            return .failure(NetworkBaseError.unexpectedEmptyData)
        }

        do {
            let value = try parse(data: payload)
            return .success(value)
        } catch {
            return .failure(error)
        }
    }

    open func parse(data _: Data) throws -> R {
        fatalError("parse should be implement by subclass")
    }
}
