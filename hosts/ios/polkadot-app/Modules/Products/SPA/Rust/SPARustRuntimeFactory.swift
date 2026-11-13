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

    /// The host session of the view this factory builds runtimes for. Every runtime, a retry's too, reports under
    /// one id, the tap's when a tap opened the view, so the Pocket event, the held card and the page stay joined.
    /// Only the first runtime is timed from the tap; a retry loads the page again later.
    struct HostSession {
        let id: UUID
        private var pendingTap: HostOpenTap?

        init(openTap: HostOpenTap?) {
            id = openTap?.sessionID ?? UUID()
            pendingTap = openTap
        }

        mutating func takeTap() -> HostOpenTap? {
            defer { pendingTap = nil }
            return pendingTap
        }
    }

    private let environment: Environment
    private weak var presentationView: ControllerBackedProtocol?
    private var hostSession: HostSession

    init(environment: Environment) {
        self.environment = environment
        hostSession = HostSession(openTap: environment.configuration.openTap)
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

        return SPARustRuntime(
            executionModel: executionModel,
            configuration: environment.configuration,
            contentLoader: environment.contentLoader,
            schemeHandlerProxy: environment.schemeHandlerProxy,
            logger: environment.rust.logger,
            hostEventRecorder: IssueMonitoringFactory.createHostEventRecorder(),
            hostSessionID: hostSession.id,
            openTap: hostSession.takeTap()
        )
    }
}
