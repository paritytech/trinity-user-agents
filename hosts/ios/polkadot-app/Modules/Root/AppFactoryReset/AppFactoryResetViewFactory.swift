#if TESTNET_FEATURE
    import Foundation

    @MainActor
    enum AppFactoryResetViewFactory {
        static func createView() -> AppFactoryResetViewProtocol {
            let wireframe = AppFactoryResetWireframe()

            let interactor = AppFactoryResetInteractor(
                resetService: AppFactoryResetService(
                    tldProvider: DotNsTldProviderFacade.shared,
                    remoteConfig: FirebaseFacade.shared,
                    mnemonicBackupHelper: MnemonicBackupHelper(),
                    logger: Logger.shared
                )
            )

            let presenter = AppFactoryResetPresenter(
                interactor: interactor,
                wireframe: wireframe
            )

            let view = AppFactoryResetViewController(presenter: presenter)

            presenter.view = view
            interactor.presenter = presenter

            BottomSheetViewFacade.setupBottomSheet(from: view, preferredHeight: nil)

            return view
        }
    }
#endif
