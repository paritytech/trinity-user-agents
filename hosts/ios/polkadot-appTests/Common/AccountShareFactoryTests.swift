import Foundation
import Testing
import UIKit

@testable import polkadot_app

@Suite("Account share factory")
struct AccountShareFactoryTests {
    @Test("Shares the QR image and one message with the download link and the username")
    func messageWithLink() throws {
        let factory = AccountShareFactory(remoteConfig: {
            makeConfig(appSharingUrl: URL(string: "https://example.com/app"))
        })

        let items = factory.createSources(username: Username(value: "alice.22"), qrImage: UIImage())
        let message = try #require(items.last as? String)

        #expect(items.count == 2)
        #expect(items.first is UIImage)
        #expect(
            message == "Hey! I’m using the Polkadot app for chat. It’s private and easy to use. "
                + "Download it at https://example.com/app and add me – my username is alice.22."
        )
    }

    @Test("Drops the link sentence when the remote config has no download link")
    func messageWithoutLink() throws {
        let factory = AccountShareFactory(remoteConfig: { makeConfig(appSharingUrl: nil) })

        let items = factory.createSources(username: Username(value: "alice.22"), qrImage: UIImage())
        let message = try #require(items.last as? String)

        #expect(items.count == 2)
        #expect(items.first is UIImage)
        #expect(
            message == "Hey! I’m using the Polkadot app for chat. It’s private and easy to use. "
                + "Add me – my username is alice.22."
        )
    }
}

private func makeConfig(appSharingUrl: URL?) -> RemoteAppConfig {
    RemoteAppConfig(
        identityBackendUrl: nil,
        ipfsGatewayUrl: nil,
        dotNsResolver: nil,
        dotNsNameRegistry: nil,
        coinageInstanceId: nil,
        fundingUrl: nil,
        offrampUrl: nil,
        accountDataStoreContract: nil,
        paymentAsset: nil,
        appSharingUrl: appSharingUrl
    )
}
