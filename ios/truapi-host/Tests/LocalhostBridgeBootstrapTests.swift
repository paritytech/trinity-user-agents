import Testing
import TrUAPIHost

struct LocalhostBridgeBootstrapTests {
    @Test
    func scriptCarriesTheAuthenticatedEndpoint() {
        let script = LocalhostBridgeBootstrap.script(port: 9955, token: "abc")

        #expect(script.contains(#"{ url: "ws://127.0.0.1:9955/?t=abc" }"#))
    }
}
