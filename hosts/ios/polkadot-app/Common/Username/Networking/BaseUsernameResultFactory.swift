import Foundation
import Operation_iOS

class BaseUsernameResultFactory<R> {
    typealias ResultType = R

    func parseReponse(from _: Data?) throws -> R {
        fatalError("Must be overriden by subsclass")
    }
}

extension BaseUsernameResultFactory: NetworkResultFactoryProtocol {
    func createResult(data: Data?, response: URLResponse?, error: Error?) -> Result<ResultType, Error> {
        if let connectionError = error {
            return .failure(connectionError)
        }

        guard let httpUrlResponse = response as? HTTPURLResponse else {
            return .failure(NetworkBaseError.unexpectedResponseObject)
        }

        guard let statusCode = BackendStatusCode(rawValue: httpUrlResponse.statusCode) else {
            return .failure(NetworkResponseError.unexpectedStatusCode)
        }

        if statusCode.isOk {
            do {
                let result = try parseReponse(from: data)

                return .success(result)
            } catch {
                return .failure(error)
            }
        } else {
            let details = data.flatMap { String(data: $0, encoding: .utf8) }

            let error = BackendApiError(statusCode: statusCode, details: details)

            return .failure(error)
        }
    }
}

final class UsernameJsonResultFactory<R: Decodable>: BaseUsernameResultFactory<R> {
    override func parseReponse(from data: Data?) throws -> R {
        guard let data else {
            throw NetworkBaseError.unexpectedEmptyData
        }

        return try JSONDecoder().decode(R.self, from: data)
    }
}

final class UsernameNoResultFactory: BaseUsernameResultFactory<Void> {
    override func parseReponse(from _: Data?) throws {
        ()
    }
}
