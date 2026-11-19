import CoreLocation
import Foundation
import Products
import Testing

@testable import polkadot_app

@Suite("OSPermissionAsker location")
struct OSPermissionAskerTests {
    @Test("location status comes from the app's location authorization")
    func locationStatus() async {
        let authorizer = FakeLocationAuthorizer(status: .denied)
        let asker = OSPermissionAsker(locationAuthorizer: authorizer)

        #expect(await asker.checkPermission(for: .location) == .denied)
    }

    @Test("a location request asks for when-in-use authorization")
    func locationRequest() async {
        let authorizer = FakeLocationAuthorizer(status: .notDetermined, grants: true)
        let asker = OSPermissionAsker(locationAuthorizer: authorizer)

        #expect(await asker.requestPermission(for: .location))
        #expect(authorizer.requests == 1)
    }

    @Test(
        "CoreLocation authorization maps onto the OS permission status",
        arguments: [
            (CLAuthorizationStatus.authorizedWhenInUse, OSPermissionStatus.allowed),
            (.authorizedAlways, .allowed),
            (.denied, .denied),
            (.restricted, .denied),
            (.notDetermined, .notDetermined)
        ]
    )
    func statusMapping(location: CLAuthorizationStatus, expected: OSPermissionStatus) {
        #expect(OSPermissionStatus(locationStatus: location) == expected)
    }
}

private final class FakeLocationAuthorizer: LocationAuthorizing, @unchecked Sendable {
    private let status: OSPermissionStatus
    private let grants: Bool
    private(set) var requests = 0

    init(status: OSPermissionStatus, grants: Bool = false) {
        self.status = status
        self.grants = grants
    }

    func status() async -> OSPermissionStatus { status }

    func requestWhenInUse() async -> Bool {
        requests += 1
        return grants
    }
}
