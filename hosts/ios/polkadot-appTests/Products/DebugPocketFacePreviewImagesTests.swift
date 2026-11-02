import Foundation
import PolkadotUI
import Testing
@testable import polkadot_app

/// The face preview fetches a face from the developer's server, so its archive images come from that same server,
/// read as a card reads them from its archive: each path is a file name, and one that climbs out of the archive is
/// refused. Reading it as a URL instead would let a face send the preview to another server.
struct DebugPocketFacePreviewImagesTests {
    @Test(arguments: [
        (
            CustomMessageWidgetNode.ImageSource.archive(path: "images/stamp.png"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/images/stamp.png"
        ),
        (
            .archive(path: "images/stamp.png"),
            "http://127.0.0.1:5173/pocket/loyalty.json?v=1#top",
            "http://127.0.0.1:5173/images/stamp.png"
        ),
        (
            .archive(path: "images/stamp.png"),
            "https://face.trycloudflare.com:8443/pocket/loyalty.json",
            "https://face.trycloudflare.com:8443/images/stamp.png"
        ),
        (
            .archive(path: "images/badges/../stamp.png"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/images/stamp.png"
        ),
        (
            .archive(path: "stamp.png?v=2"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/stamp.png%3Fv=2"
        ),
        (
            .archive(path: "x/%2e%2e/stamp.png"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/x/%252e%252e/stamp.png"
        ),
        (
            .archive(path: "https://example.com/stamp.png"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/https:/example.com/stamp.png"
        ),
        (
            .archive(path: "//example.com/stamp.png"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/example.com/stamp.png"
        ),
        (
            .archive(path: "http://192.168.1.1/stamp.png"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/http:/192.168.1.1/stamp.png"
        ),
        (
            .archive(path: "file:///etc/hosts"),
            "http://127.0.0.1:5173/pocket/loyalty.json",
            "http://127.0.0.1:5173/file:/etc/hosts"
        ),
        (.archive(path: "../secret.png"), "http://127.0.0.1:5173/pocket/loyalty.json", nil),
        (.archive(path: "images/../../secret.png"), "http://127.0.0.1:5173/pocket/loyalty.json", nil),
        (.bulletin(cid: "bafycid"), "http://127.0.0.1:5173/pocket/loyalty.json", "https://gateway.test/ipfs/bafycid")
    ] as [(CustomMessageWidgetNode.ImageSource, String, String?)])
    func readsAnImageFromWhereACardWould(source: CustomMessageWidgetNode.ImageSource, face: String, expected: String?) {
        let url = DebugPocketFacePreviewImages.url(
            for: source,
            faceURL: URL(string: face)!,
            gateway: { URL(string: "https://gateway.test/ipfs/\($0)") }
        )

        #expect(url?.absoluteString == expected)
    }
}
