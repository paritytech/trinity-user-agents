import Testing
import UIKit
@testable import polkadot_app

/// A host screen opens for a product only when the user can see who is asking,
/// so a product hidden behind the app, another page or a sheet cannot pull up
/// the camera.
@MainActor
struct ProductVisibilityTests {
    @Test("An App may ask only while its own page is the top screen of the app in front")
    func appNeedsItsPageOnTop() async throws {
        let page = StubProductPage(productId: "shop.dot")
        let window = try makeTestWindow(root: page)
        let application = StubApplication()
        let visibility = ProductVisibility(application: application, window: { window })

        let onTop = visibility.canShow(productId: "shop.dot", executionKind: .app)
        let otherProduct = visibility.canShow(productId: "other.dot", executionKind: .app)
        application.applicationState = .inactive
        let appBehind = visibility.canShow(productId: "shop.dot", executionKind: .app)
        application.applicationState = .active
        await present(UIViewController(), over: page)
        let covered = visibility.canShow(productId: "shop.dot", executionKind: .app)

        #expect([onTop, otherProduct, appBehind, covered] == [true, false, false, false])
    }

    @Test("A Worker has no page, so the app in front is enough")
    func workerNeedsTheAppInFront() throws {
        let window = try makeTestWindow(root: UIViewController())
        let application = StubApplication()
        let visibility = ProductVisibility(application: application, window: { window })

        let inFront = visibility.canShow(productId: "shop.dot", executionKind: .worker)
        application.applicationState = .background
        let inBackground = visibility.canShow(productId: "shop.dot", executionKind: .worker)

        #expect([inFront, inBackground] == [true, false])
    }

    @Test("A Widget's page may sit inside the screen that shows it, as on an open Pocket card")
    func widgetPageInsideItsScreen() throws {
        let screen = UIViewController()
        screen.addChild(StubProductPage(productId: "shop.dot"))
        let window = try makeTestWindow(root: screen)
        let visibility = ProductVisibility(application: StubApplication(), window: { window })

        #expect(visibility.canShow(productId: "shop.dot", executionKind: .widget))
    }

    @Test("Only a screen still opening or closing is waited for, so a hidden product is refused at once")
    func hiddenProductIsNotWaitedFor() async throws {
        let page = StubProductPage(productId: "shop.dot")
        let window = try makeTestWindow(root: page)
        let application = StubApplication()
        let visibility = ProductVisibility(application: application, window: { window })
        await present(UIViewController(), over: page)
        let clock = ContinuousClock()

        let covered = await clock.measure {
            _ = await visibility.waitUntilVisible(productId: "shop.dot", executionKind: .app)
        }
        application.applicationState = .background
        let background = await clock.measure {
            _ = await visibility.waitUntilVisible(productId: "shop.dot", executionKind: .worker)
        }

        #expect(covered < .milliseconds(300))
        #expect(background < .milliseconds(300))
    }

    @Test("A screen closing over the page holds the request until it has gone")
    func closingScreenIsWaitedFor() async throws {
        let page = StubProductPage(productId: "shop.dot")
        let window = try makeTestWindow(root: page)
        let visibility = ProductVisibility(application: StubApplication(), window: { window })
        await present(UIViewController(), over: page)

        page.dismiss(animated: true)
        let visible = await visibility.waitUntilVisible(productId: "shop.dot", executionKind: .app)

        #expect(visible)
    }
}
