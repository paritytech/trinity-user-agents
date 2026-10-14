import Foundation
import Foundation_iOS

@MainActor
public enum TitleDetailsSheetViewFactory {
    public static func createView(
        from viewModel: TitleDetailsSheetViewModel,
        styler: MessageSheetStyling,
        allowsSwipeDown: Bool = true,
        localizationManager: LocalizationManagerProtocol? = nil
    ) -> MessageSheetViewProtocol {
        let wireframe = MessageSheetWireframe()

        let presenter = MessageSheetPresenter(wireframe: wireframe)

        let view = TitleDetailsSheetViewController(
            presenter: presenter,
            viewModel: viewModel,
            styler: styler,
            localizationManager: localizationManager
        )

        view.allowsSwipeDown = allowsSwipeDown

        presenter.view = view

        return view
    }
}
