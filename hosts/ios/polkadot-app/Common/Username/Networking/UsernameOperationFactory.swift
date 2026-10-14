import Foundation
import Operation_iOS
import StructuredConcurrency
import UniqueDevice

protocol UsernameOperationFactoryProtocol {
    func availableUsernameWrapper(for username: String) -> CompoundOperationWrapper<UsernameAvailableType>
    func searchUsernameWrapper(for username: UsernameRequestModel) -> CompoundOperationWrapper<[UsernameResponseModel]>
    func claimUsername(
        using request: RegisterUsernameParameters
    ) -> CompoundOperationWrapper<UsernameResponse>
    func attester() -> CompoundOperationWrapper<UsernameAttester>
}

final class UsernameOperationFactory {
    private let tokenProvider: JWTTokenProviding

    init(tokenProvider: JWTTokenProviding) {
        self.tokenProvider = tokenProvider
    }
}

extension UsernameOperationFactory {
    func createJWTAuthorizedRequestWrapper<R>(
        endpoint: URLConvertible,
        responseFactory: BaseUsernameResultFactory<R>,
        tokenProvider: JWTTokenProviding
    ) -> CompoundOperationWrapper<R> {
        let operation = AsyncTaskOperation<R> {
            try await tokenProvider.withAuthorizedToken { token in
                let requestFactory = BlockNetworkRequestFactory {
                    var request = URLRequest(url: endpoint.url)
                    request.httpMethod = endpoint.httpMethod
                    request.setValue(
                        HttpContentType.json.rawValue,
                        forHTTPHeaderField: HttpHeaderKey.contentType.rawValue
                    )

                    let modifier = BearerTokenRequestModifier(token: token)
                    try modifier.visit(request: &request)

                    if let bodyParams = endpoint.params {
                        request.httpBody = try JSONEncoder().encode(bodyParams)
                    }

                    return request
                }

                let networkOp = NetworkOperation(
                    requestFactory: requestFactory,
                    resultFactory: AnyNetworkResultFactory(factory: responseFactory)
                )

                return try await networkOp.asyncExecute()
            }
        }

        return CompoundOperationWrapper(targetOperation: operation)
    }

    func createJWTAndDeviceCheckRequestWrapper<R>(
        endpoint: URLConvertible,
        responseFactory: BaseUsernameResultFactory<R>,
        tokenProvider: JWTTokenProviding
    ) -> CompoundOperationWrapper<R> {
        let devcheck = DeviceCheckProvider().deviceTokenModifier()

        let operation = AsyncTaskOperation<R> { [devcheck] in
            let devMod = try devcheck.targetOperation.extractNoCancellableResultData()

            return try await tokenProvider.withAuthorizedToken { token in
                let requestFactory = BlockNetworkRequestFactory {
                    var request = URLRequest(url: endpoint.url)
                    request.httpMethod = endpoint.httpMethod
                    request.setValue(
                        HttpContentType.json.rawValue,
                        forHTTPHeaderField: HttpHeaderKey.contentType.rawValue
                    )

                    let bearerModifier = BearerTokenRequestModifier(token: token)
                    try bearerModifier.visit(request: &request)
                    try devMod.visit(request: &request)

                    if let bodyParams = endpoint.params {
                        request.httpBody = try JSONEncoder().encode(bodyParams)
                    }

                    return request
                }

                let networkOp = NetworkOperation(
                    requestFactory: requestFactory,
                    resultFactory: AnyNetworkResultFactory(factory: responseFactory)
                )

                return try await networkOp.asyncExecute()
            }
        }

        operation.addDependency(devcheck.targetOperation)

        return devcheck.insertingTail(operation: operation)
    }

    func createGenericRequestWrapper<R>(
        endpoint: URLConvertible,
        responseFactory: BaseUsernameResultFactory<R>
    ) -> NetworkOperation<R> {
        let requestFactory = BlockNetworkRequestFactory {
            var request = URLRequest(url: endpoint.url)
            request.httpMethod = endpoint.httpMethod
            request.setValue(
                HttpContentType.json.rawValue,
                forHTTPHeaderField: HttpHeaderKey.contentType.rawValue
            )

            if let bodyParams = endpoint.params {
                request.httpBody = try JSONEncoder().encode(bodyParams)
            }

            return request
        }

        return NetworkOperation(
            requestFactory: requestFactory,
            resultFactory: AnyNetworkResultFactory(factory: responseFactory)
        )
    }
}

extension UsernameOperationFactory: UsernameOperationFactoryProtocol {
    func attester() -> CompoundOperationWrapper<UsernameAttester> {
        createJWTAuthorizedRequestWrapper(
            endpoint: UsernameApi.V1.attester,
            responseFactory: UsernameJsonResultFactory<UsernameAttester>(),
            tokenProvider: tokenProvider
        )
    }

    func availableUsernameWrapper(for username: String) -> CompoundOperationWrapper<UsernameAvailableType> {
        let endpoint = UsernameApi.V1.available(username)

        let requestWrapper = createJWTAuthorizedRequestWrapper(
            endpoint: endpoint,
            responseFactory: UsernameJsonResultFactory<UsernameAvailableResponse>(),
            tokenProvider: tokenProvider
        )

        let mappingOperation = ClosureOperation<UsernameAvailableType> {
            do {
                let usernames = try requestWrapper.targetOperation.extractNoCancellableResultData().usernames
                if !username.isEmpty, let status = usernames[username] {
                    return status
                }
                return .error("Unknown error")
            } catch {
                return .error(error.localizedDescription)
            }
        }

        mappingOperation.addDependency(requestWrapper.targetOperation)

        return requestWrapper.insertingTail(operation: mappingOperation)
    }

    func searchUsernameWrapper(for username: UsernameRequestModel)
        -> CompoundOperationWrapper<[UsernameResponseModel]> {
        let endpoint = UsernameApi.V1.search(username)
        let requestWrapper = createJWTAuthorizedRequestWrapper(
            endpoint: endpoint,
            responseFactory: UsernameJsonResultFactory<UsernameSearchResult>(),
            tokenProvider: tokenProvider
        )

        let mappingOperation = ClosureOperation<[UsernameResponseModel]> {
            do {
                return try requestWrapper.targetOperation.extractNoCancellableResultData().usernames
            } catch let error as BackendApiError where error.statusCode == .notFound {
                return []
            }
        }

        mappingOperation.addDependency(requestWrapper.targetOperation)

        return requestWrapper.insertingTail(operation: mappingOperation)
    }

    func claimUsername(
        using request: RegisterUsernameParameters
    ) -> CompoundOperationWrapper<UsernameResponse> {
        createJWTAndDeviceCheckRequestWrapper(
            endpoint: UsernameApi.V1.register(request),
            responseFactory: UsernameJsonResultFactory<UsernameResponse>(),
            tokenProvider: tokenProvider
        )
    }
}
