import Foundation
import Foundation_iOS
import UIKitExt
import ChainRegistry

@MainActor
protocol ExplorerPresentable: WebPresentable {
    func show(extrinsicHash: String, in exlorer: ChainModel.Explorer, from view: ControllerBackedProtocol)
}

extension ExplorerPresentable {
    func show(
        extrinsicHash: String,
        in exlorer: ChainModel.Explorer,
        from view: ControllerBackedProtocol
    ) {
        guard
            let template = exlorer.extrinsic,
            let url = try? URLBuilder(urlTemplate: template).buildParameterURL(extrinsicHash) else {
            return
        }

        showWeb(url: url, from: view, style: .init(mode: .automatic))
    }
}
