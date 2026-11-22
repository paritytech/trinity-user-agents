import Foundation
import AVFoundation
import Testing
import Products
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIProductExecutionDeviceCapabilityTests {
    @Test(
        arguments: [JSDeviceCapability.camera, .microphone],
        [OSPermissionStatus.allowed, .denied, .notDetermined]
    )
    func directCaptureRemainsDenied(capability: JSDeviceCapability, status: OSPermissionStatus) async throws {
        let osAsker = MockOSPermissionAsker()
        osAsker.statusToReturn = status
        let execution = MockProductExecution()
        execution.devicePermissionGranted = true
        let handler = osAsker.makeDeviceCapabilityHandler(execution: execution)

        #expect(try await handler(capability) == .denied)
        #expect(execution.devicePermissionRequests.isEmpty)
        #expect(osAsker.checkedCapabilities.isEmpty)
        #expect(osAsker.requestedCapabilities.isEmpty)
    }

    @Test(arguments: [true, false])
    func motionResolvesThroughProductDecision(granted: Bool) async throws {
        let osAsker = MockOSPermissionAsker()
        let execution = MockProductExecution()
        execution.devicePermissionGranted = granted
        let handler = osAsker.makeDeviceCapabilityHandler(execution: execution)

        #expect(try await handler(.motion) == (granted ? .allowed : .denied))
        #expect(execution.devicePermissionRequests == [.motion])
        #expect(osAsker.checkedCapabilities.isEmpty)
        #expect(osAsker.requestedCapabilities.isEmpty)
    }

    @Test func restrictedCaptureIsDeniedByOS() {
        let statuses: [AVAuthorizationStatus] = [.authorized, .notDetermined, .denied, .restricted]

        #expect(statuses.map { OSPermissionStatus(mediaStatus: $0) } == [
            .allowed, .notDetermined, .denied, .denied
        ])
    }
}
