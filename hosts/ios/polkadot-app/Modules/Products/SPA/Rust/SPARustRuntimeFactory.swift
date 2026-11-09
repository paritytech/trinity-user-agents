import struct TrUAPIHost.ProductId
import Foundation
import IssueMonitoring
import Products
import UIKitExt

/// Builds one rust SPA runtime (TrUAPI core + localhost ws-bridge) per call.
final class SPARustRuntimeFactory {
    struct Environment {
        let rust: RustRuntimeEnvironment
        let configuration: SPAConfiguration
        let contentLoader: ProductContentLoading
        let schemeHandlerProxy: SchemeHandlerProxy
        let routers: ProductRoutersFacadeProtocol
    }

    private let environment: Environment
    private weak var presentationView: ControllerBackedProtocol?
    private var pendingOpenTap: HostOpenTap?

    init(environment: Environment) {
        self.environment = environment
        pendingOpenTap = environment.configuration.openTap
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
    func createRuntime(for productId: ProductId) throws -> SPARuntimeProtocol {
        if let presentationView {
            environment.routers.setPresentationView(presentationView)
        }

        let executionModel = try environment.rust.makeSPAExecution(
            productId: productId,
            routers: environment.routers
        )

        // Only the first runtime is timed from the tap; a retry opens the page again later.
        let openTap = pendingOpenTap
        pendingOpenTap = nil

        return SPARustRuntime(
            executionModel: executionModel,
            configuration: environment.configuration,
            contentLoader: environment.contentLoader,
            schemeHandlerProxy: environment.schemeHandlerProxy,
            logger: environment.rust.logger,
            hostEventRecorder: IssueMonitoringFactory.createHostEventRecorder(),
            openTap: openTap
        )
    }
}
