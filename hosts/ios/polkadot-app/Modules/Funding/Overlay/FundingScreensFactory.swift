import UIKit
import Products
import UIKitExt

/// Builds the funding screens the main tab bar shows for the core, presenting
/// from `presentationView`, and hands their router to the activity center.
@MainActor
enum FundingScreensFactory {
    static func createScreens(
        serviceCoordinator: ServiceCoordinatorProtocol,
        presentationView: ControllerBackedProtocol
    ) -> FundingScreens {
        let router = ProductsRouter()
        router.setPresentationView(presentationView)

        let activityCenter = serviceCoordinator.fundingActivityCenter
        let screens = FundingScreens(
            service: serviceCoordinator.fundingService,
            environment: AppFundingEnvironment(router: router),
            notice: FundingFrameNoticePresenter(application: UIApplication.shared),
            tracker: activityCenter,
            logger: Logger.shared
        )
        activityCenter.attach(router: screens.router)

        return screens
    }
}
