import Foundation
import PolkadotUI
import Products

struct SPATabChipViewModel {
    let id: UUID
    let name: String
    let icon: AnyImageViewModel?
}

@MainActor
final class SPATabChipViewModelFactory {
    private let flowStateProvider: any SPAFlowStateProviding

    init(flowStateProvider: any SPAFlowStateProviding) {
        self.flowStateProvider = flowStateProvider
    }

    func createViewModels(for tabs: [SPATab]) -> [SPATabChipViewModel] {
        tabs.map { tab in
            SPATabChipViewModel(id: tab.id, name: name(for: tab), icon: icon(for: tab))
        }
    }
}

private extension SPATabChipViewModelFactory {
    func name(for tab: SPATab) -> String {
        flowStateProvider.flowState().hostProvider.host(rawString: tab.dotDomain.rawValue)?.name
            ?? String(localized: .Products.productTabsNotAvailable)
    }

    func icon(for tab: SPATab) -> AnyImageViewModel? {
        AnyImageViewModel(flowStateProvider.flowState().iconViewModelFactory.createViewModel(for: tab.dotDomain))
    }
}
