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
    private let voipToken: @Sendable () async -> Data?
    private let routes: ProductPushRoutes
    private let relay: ProductPushRelaying

    init(
        deviceToken: @escaping @Sendable () -> Data?,
        voipToken: @escaping @Sendable () async -> Data? = { nil },
        routes: ProductPushRoutes = ProductPushRoutes(),
        relay: ProductPushRelaying
    ) {
        self.deviceToken = deviceToken
        self.voipToken = voipToken
        self.routes = routes
        self.relay = relay
    }

    func sendPush(productId _: String, delivery: HostPushDelivery) async throws -> Bool {
        try await relay.notify(NotifyRequestParameters(
            deviceToken: delivery.token,
            pushId: delivery.pushId,
            bundlerId: Bundle.main.bundleIdentifier ?? "",
            platform: delivery.service == .apns ? "ios" : "android",
            message: delivery.message,
            voip: delivery.voip
        ))
    }

    func setChatPushRoutes(productId: String, routes: [HostChatPushRoute]) async throws {
        var byPushId: [String: ProductPushRoute] = [:]
        for route in routes {
            byPushId[route.pushId] = ProductPushRoute(
                productId: productId,
                roomId: route.roomId,
                key: route.key,
                chatTitle: route.title
            )
        }
        try self.routes.replaceChatRoutes(productId: productId, routes: byPushId)
    }

    func pushTokens() async throws -> HostPushTokens {
        HostPushTokens(service: .apns, token: deviceToken()?.toHex(), voipToken: await voipToken()?.toHex())
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
