import Foundation
import Products
import UIKitExt

/// Builds one rust SPA runtime (TrUAPI core + localhost ws-bridge) per call.
final class SPARustRuntimeFactory {
    struct Environment {
        let makeRust: @MainActor () async throws -> RustRuntimeEnvironment
        let configuration: SPAConfiguration
        let dotNsResolver: DotNsResolverProtocol
        let productResolver: ProductResolving
        let schemeHandlerProxy: SchemeHandlerProxy
        let routers: ProductRoutersFacadeProtocol
    }

    private let environment: Environment
    private weak var presentationView: ControllerBackedProtocol?

    init(environment: Environment) {
        self.environment = environment
    }

    /// The view runtime prompts anchor to; attached to the routers on every
    /// `createRuntime`.
    @MainActor
    func setPresentationView(_ view: ControllerBackedProtocol) {
        presentationView = view
    }
}

extension SPARustRuntimeFactory: SPARuntimeFactoryProtocol {
    @MainActor
    func createRuntime(for productId: ProductId) async throws -> SPARuntimeProtocol {
        if let presentationView {
            environment.routers.setPresentationView(presentationView)
        }

        let rust = try await environment.makeRust()
        try Task.checkCancellation()
        let executionModel = try rust.makeSPAExecution(
            productId: productId,
            routers: environment.routers
        )

        return SPARustRuntime(
            executionModel: executionModel,
            configuration: environment.configuration,
            dotNsResolver: environment.dotNsResolver,
            productResolver: environment.productResolver,
            schemeHandlerProxy: environment.schemeHandlerProxy,
            logger: rust.logger
        )
    }
}
