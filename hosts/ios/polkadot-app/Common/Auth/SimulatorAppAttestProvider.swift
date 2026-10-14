#if targetEnvironment(simulator) && (DEBUG || E2E_TEST)
    import Foundation
    import Operation_iOS
    import UniqueDevice

    /// Simulator-only AppAttest provider. App Attest cannot produce an assertion on the
    /// Simulator, but the dev backend accepts a proof-only token request. This provider
    /// fetches a real backend challenge (so the downstream sr25519 proof validates) and
    /// sets it as the `Auth-Challenge` header, omitting attestation headers.
    /// `BackendAuthProofRequestModifier` adds `Auth-ClientId` / `Auth-ClientProof` after.
    final class SimulatorAppAttestProvider: AppAttestProviding {
        private struct ChallengeResponse: Decodable { let challenge: String }

        private struct Modifier: HttpRequestModifier {
            let body: Data?
            let challengeBase64: String
            func visit(request: inout URLRequest) throws {
                request.httpBody = body
                request.setValue(challengeBase64, forHTTPHeaderField: "Auth-Challenge")
            }
        }

        private let urlSession: any HTTPDataLoading
        init(
            urlSession: any HTTPDataLoading = URLSession.shared
        ) {
            self.urlSession = urlSession
        }

        func setup() {}

        func appAttestModifier(
            for bodyDataClosure: @escaping () throws -> Data?,
            clientIdClosure _: (() throws -> Data)?
        ) -> CompoundOperationWrapper<HttpRequestModifier> {
            let urlSession = urlSession
            var task: Task<Void, Never>?
            let operation = AsyncClosureOperation<HttpRequestModifier>(
                operationClosure: { callback in
                    task = Task {
                        do {
                            let body = try bodyDataClosure()
                            let challenge = try await Self.fetchChallenge(using: urlSession)
                            callback(.success(Modifier(body: body, challengeBase64: challenge)))
                        } catch {
                            callback(.failure(error))
                        }
                    }
                },
                cancelationClosure: { task?.cancel() }
            )
            return CompoundOperationWrapper(targetOperation: operation)
        }

        private static func fetchChallenge(using urlSession: any HTTPDataLoading) async throws -> String {
            var request = URLRequest(url: AuthApi.challenge.url)
            request.httpMethod = HttpMethod.post.rawValue
            request.setValue(HttpContentType.json.rawValue, forHTTPHeaderField: HttpHeaderKey.contentType.rawValue)

            let (data, urlResponse) = try await urlSession.data(for: request)
            guard let http = urlResponse as? HTTPURLResponse else { throw NetworkBaseError.unexpectedResponseObject }

            guard
                let status = BackendStatusCode(rawValue: http.statusCode)
            else {
                throw NetworkResponseError.unexpectedStatusCode
            }
            guard status.isOk else {
                throw BackendApiError(
                    statusCode: status,
                    details: String(data: data, encoding: .utf8)
                )
            }
            return try JSONDecoder().decode(ChallengeResponse.self, from: data).challenge
        }
    }
#endif
