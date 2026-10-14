import UIKit
import Products

@MainActor
protocol SPAControllerPooling: AnyObject {
    func controller(for id: UUID) -> SPAViewProtocol?
    func makeController(for tab: SPATab) -> SPAViewProtocol?
    func removeController(for id: UUID)
}

@MainActor
final class SPAControllerPool: SPAControllerPooling {
    private var controllers: [UUID: SPAViewProtocol] = [:]
    private let flowStateProvider: any SPAFlowStateProviding

    init(flowStateProvider: any SPAFlowStateProviding) {
        self.flowStateProvider = flowStateProvider
    }

    func controller(for id: UUID) -> SPAViewProtocol? { controllers[id] }

    func makeController(for tab: SPATab) -> SPAViewProtocol? {
        if let existing = controllers[tab.id] { return existing }

        let flowState = flowStateProvider.flowState()

        guard let page = tab.makeProductPage(hostProvider: flowState.hostProvider) else { return nil }

        guard let view = SPAViewFactory.createView(
            page: page,
            flowState: flowState,
            isBrowserTab: true,
            browserTabId: tab.id
        ) else {
            return nil
        }
        controllers[tab.id] = view
        return view
    }

    func removeController(for id: UUID) { controllers[id] = nil }
}
