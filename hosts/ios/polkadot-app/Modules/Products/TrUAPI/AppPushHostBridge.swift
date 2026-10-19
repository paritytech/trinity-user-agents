import Foundation
import Operation_iOS
import SubstrateSdk
import TrUAPIHost

/// The push service of products: the APNs token of this device, a route for each product room (push id and key in
/// the keychain that the notification service extension reads), and the push relay of the backend, which this app
/// calls with its own JWT, as the native chat does. The product never sees the JWT, and the relay sees only the
/// token, the push id and the ciphertext.
final class AppPushHostBridge: PushHostBridge, @unchecked Sendable {
    private let deviceToken: @Sendable () -> Data?
    private let routes: ProductPushRoutes
    private let relay: ProductPushRelaying

    init(
        deviceToken: @escaping @Sendable () -> Data?,
        routes: ProductPushRoutes = ProductPushRoutes(),
        relay: ProductPushRelaying
    ) {
        self.deviceToken = deviceToken
        self.routes = routes
        self.relay = relay
    }

    func pushRoute(productId: String, roomId: String) async throws -> HostPushRoute? {
        guard let token = deviceToken() else { return nil }

        let pushId: String
        let key: Data
        if let existing = routes.pushId(productId: productId, roomId: roomId),
           let route = routes.route(pushId: existing) {
            pushId = existing
            key = route.key
        } else {
            pushId = try Data.randomOrError(of: 16).toHex()
            key = try Data.randomOrError(of: 32)
            try routes.save(ProductPushRoute(productId: productId, roomId: roomId, key: key), pushId: pushId)
        }
        return HostPushRoute(service: .apns, token: token.toHex(), pushId: pushId, key: key)
    }

    func sendPush(productId _: String, delivery: HostPushDelivery) async throws -> Bool {
        try await relay.notify(NotifyRequestParameters(
            deviceToken: delivery.token,
            pushId: delivery.pushId,
            bundlerId: Bundle.main.bundleIdentifier ?? "",
            platform: delivery.service == .apns ? "ios" : "android",
            message: delivery.message,
            voip: false
        ))
    }
}

/// Sends one push through the push relay of the backend.
protocol ProductPushRelaying: Sendable {
    /// Answers whether the push service took the push.
    func notify(_ parameters: NotifyRequestParameters) async throws -> Bool
}

/// The push relay of the backend (`POST /api/v1/notify`), with the JWT of this app.
struct BackendProductPushRelay: ProductPushRelaying {
    let tokenProvider: JWTTokenProviding

    func notify(_ parameters: NotifyRequestParameters) async throws -> Bool {
        let response: NotifyResponse = try await tokenProvider.withAuthorizedToken { token in
            let endpoint = UsernameApi.V1.notify(parameters)
            let requestFactory = BlockNetworkRequestFactory {
                var request = URLRequest(url: endpoint.url)
                request.httpMethod = endpoint.httpMethod
                request.setValue(HttpContentType.json.rawValue, forHTTPHeaderField: HttpHeaderKey.contentType.rawValue)
                try BearerTokenRequestModifier(token: token).visit(request: &request)
                if let bodyParams = endpoint.params {
                    request.httpBody = try JSONEncoder().encode(bodyParams)
                }
                return request
            }
            return try await NetworkOperation(
                requestFactory: requestFactory,
                resultFactory: AnyNetworkResultFactory(factory: UsernameJsonResultFactory<NotifyResponse>())
            ).asyncExecute()
        }
        return response.success
    }
}
