import Foundation
import Testing
import struct TrUAPIHost.ProductId
import func TrUAPIHost.productContentScheme
@testable import polkadot_app

struct ProductRoomFooterWebPageTests {
    private let product = try! ProductId("dim2.testnet")

    @Test func aPathLoadsFromTheProductsOwnOrigin() throws {
        let url = try #require(ProductRoomFooterWebPage.url(
            path: "slide-footer.html?label=Slide%20to%20register&action=jollity.register",
            productId: product
        ))

        #expect(url.scheme == productContentScheme())
        #expect(url.host == "dim2.testnet")
        #expect(url.path == "/slide-footer.html")
        #expect(url.query == "label=Slide%20to%20register&action=jollity.register")
    }

    @Test func aNestedPathStaysUnderTheOrigin() throws {
        let url = try #require(ProductRoomFooterWebPage.url(path: "footer/slide.html#idle", productId: product))

        #expect(url.absoluteString == "\(productContentScheme())://dim2.testnet/footer/slide.html#idle")
    }

    @Test(arguments: [
        "https://evil.invalid/page.html",
        "javascript:alert(1)",
        "//evil.invalid/page.html",
        "/slide-footer.html",
        "polkadot://other.testnet/page.html",
        "data:text/html,hi",
        "a\\b.html",
        "",
        "   "
    ])
    func aPathOffTheOriginIsRefused(path: String) {
        #expect(ProductRoomFooterWebPage.url(path: path, productId: product) == nil)
    }

    @Test func onlyTheProductOriginCounts() throws {
        let scheme = productContentScheme()
        #expect(try ProductRoomFooterWebPage.isProductOrigin(
            #require(URL(string: "\(scheme)://dim2.testnet/a.html")),
            productId: product
        ))
        #expect(try ProductRoomFooterWebPage.isProductOrigin(
            #require(URL(string: "\(scheme)://DIM2.testnet/a.html")),
            productId: product
        ))
        #expect(try !ProductRoomFooterWebPage.isProductOrigin(
            #require(URL(string: "\(scheme)://peopl.testnet/a.html")),
            productId: product
        ))
        #expect(try !ProductRoomFooterWebPage.isProductOrigin(
            #require(URL(string: "https://dim2.testnet/a.html")),
            productId: product
        ))
        #expect(try !ProductRoomFooterWebPage.isProductOrigin(
            #require(URL(string: "\(scheme)://user@dim2.testnet/a.html")),
            productId: product
        ))
        #expect(try !ProductRoomFooterWebPage.isProductOrigin(
            #require(URL(string: "\(scheme)://dim2.testnet:8080/a.html")),
            productId: product
        ))
    }

    @Test func theHeightIsCapped() {
        #expect(ProductRoomFooterWebPage.height(104) == 104)
        #expect(ProductRoomFooterWebPage.height(10_000) == ProductRoomFooterWebPage.maximumHeight)
        #expect(ProductRoomFooterWebPage.height(1) == ProductRoomFooterWebPage.minimumHeight)
        #expect(ProductRoomFooterWebPage.height(reported: 96) == 96)
        #expect(ProductRoomFooterWebPage.height(reported: 999) == ProductRoomFooterWebPage.maximumHeight)
        #expect(ProductRoomFooterWebPage.height(reported: nil) == nil)
        #expect(ProductRoomFooterWebPage.height(reported: 0) == nil)
        #expect(ProductRoomFooterWebPage.height(reported: .nan) == nil)
    }
}

struct ProductFooterMessageTests {
    @Test func anActionCarriesItsIdAndNoPayload() {
        let message = ProductFooterMessage(body: ["kind": "action", "actionId": "jollity.register"] as [String: Any])

        #expect(message == .action(actionId: "jollity.register", payload: nil))
    }

    @Test func anActionKeepsATextPayload() {
        let message = ProductFooterMessage(
            body: ["kind": "action", "actionId": "go", "payload": "seat-3"] as [String: Any]
        )

        #expect(message == .action(actionId: "go", payload: "seat-3"))
    }

    @Test func anObjectPayloadTravelsAsItsJSON() {
        let message = ProductFooterMessage(
            body: ["kind": "action", "actionId": "go", "payload": ["seat": 3]] as [String: Any]
        )

        #expect(message == .action(actionId: "go", payload: #"{"seat":3}"#))
    }

    @Test func aMessageMayArriveAsJSONText() {
        let message = ProductFooterMessage(body: #"{"kind":"action","actionId":"jollity.register"}"#)

        #expect(message == .action(actionId: "jollity.register", payload: nil))
    }

    @Test func anEmptyPayloadIsNoPayload() {
        let message = ProductFooterMessage(
            body: ["kind": "action", "actionId": "go", "payload": ""] as [String: Any]
        )

        #expect(message == .action(actionId: "go", payload: nil))
    }

    @Test func anActionWithoutAUsableIdIsDropped() {
        #expect(ProductFooterMessage(body: ["kind": "action"] as [String: Any]) == nil)
        #expect(ProductFooterMessage(body: ["kind": "action", "actionId": ""] as [String: Any]) == nil)
        #expect(ProductFooterMessage(body: ["kind": "action", "actionId": 7] as [String: Any]) == nil)
        let long = String(repeating: "a", count: ProductFooterMessage.actionIdMaxBytes + 1)
        #expect(ProductFooterMessage(body: ["kind": "action", "actionId": long] as [String: Any]) == nil)
    }

    @Test func anOversizedPayloadIsDropped() {
        let payload = String(repeating: "a", count: ProductFooterMessage.payloadMaxBytes + 1)

        #expect(ProductFooterMessage(
            body: ["kind": "action", "actionId": "go", "payload": payload] as [String: Any]
        ) == nil)
    }

    @Test func theOtherKindsAreRead() {
        #expect(ProductFooterMessage(body: ["kind": "haptic", "style": "success"] as [String: Any]) ==
            .haptic(style: "success"))
        #expect(ProductFooterMessage(body: ["kind": "haptic"] as [String: Any]) == .haptic(style: "success"))
        #expect(ProductFooterMessage(body: ["kind": "overlay", "on": true] as [String: Any]) == .overlay(isOn: true))
        #expect(ProductFooterMessage(body: ["kind": "overlay", "on": false] as [String: Any]) == .overlay(isOn: false))
        #expect(ProductFooterMessage(body: ["kind": "ready", "height": 104] as [String: Any]) == .ready(height: 104))
        #expect(ProductFooterMessage(body: ["kind": "ready"] as [String: Any]) == .ready(height: nil))
    }

    @Test func anythingElseIsDropped() {
        #expect(ProductFooterMessage(body: ["kind": "navigate", "url": "https://evil.invalid"] as [String: Any]) == nil)
        #expect(ProductFooterMessage(body: ["kind": "overlay"] as [String: Any]) == nil)
        #expect(ProductFooterMessage(body: 42) == nil)
        #expect(ProductFooterMessage(body: "not json") == nil)
        #expect(ProductFooterMessage(body: ["actionId": "go"] as [String: Any]) == nil)
    }

    @Test func theHostSpeaksThroughAnOptionalReceiver() {
        #expect(ProductFooterHostEvent.reset.json == #"{"kind":"reset"}"#)
        #expect(ProductFooterHostEvent.state("confirmed").json == #"{"kind":"state","state":"confirmed"}"#)
        #expect(ProductFooterHostEvent.reset.script
            .hasPrefix("window.productFooter && typeof window.productFooter.receive === 'function'"))
        #expect(ProductFooterHostEvent.reset.script.hasSuffix(#"window.productFooter.receive({"kind":"reset"});"#))
    }
}
