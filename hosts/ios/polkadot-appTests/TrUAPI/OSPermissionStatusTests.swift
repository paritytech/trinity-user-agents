import Foundation
import AVFoundation
import Testing
import Products
import TrUAPIHost
@testable import polkadot_app

struct OSPermissionStatusTests {
    @Test func restrictedCaptureIsDeniedByOS() {
        let statuses: [AVAuthorizationStatus] = [.authorized, .notDetermined, .denied, .restricted]

        #expect(statuses.map { OSPermissionStatus(mediaStatus: $0) } == [
            .allowed, .notDetermined, .denied, .denied
        ])
    }
}
