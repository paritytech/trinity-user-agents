import Foundation
import AVFoundation
import Testing
import Products
import TrUAPIHost
@testable import polkadot_app

struct TrUAPIProductExecutionDeviceCapabilityTests {
    @Test(arguments: [JSDeviceCapability.camera, .microphone])
    func allowedWithoutRepeatingProductConsent(capability: JSDeviceCapability) async throws {
        let osAsker = MockOSPermissionAsker()
        osAsker.statusToReturn = .allowed
        let handler = osAsker.makeDeviceCapabilityHandler(execution: MockProductExecution())

        #expect(try await handler(capability) == .allowed)
        #expect(osAsker.checkedCapabilities == [capability.deviceCapabilityType])
        #expect(osAsker.requestedCapabilities.isEmpty)
    }

    @Test(arguments: [JSDeviceCapability.camera, .microphone])
    func deniedByOSWithoutPrompting(capability: JSDeviceCapability) async throws {
        let osAsker = MockOSPermissionAsker()
        osAsker.statusToReturn = .denied
        let handler = osAsker.makeDeviceCapabilityHandler(execution: MockProductExecution())

        #expect(try await handler(capability) == .denied)
        #expect(osAsker.checkedCapabilities == [capability.deviceCapabilityType])
        #expect(osAsker.requestedCapabilities.isEmpty)
    }

    @Test(arguments: [JSDeviceCapability.camera, .microphone], [true, false])
    func requestsOnlyUndeterminedOSConsent(capability: JSDeviceCapability, granted: Bool) async throws {
        let osAsker = MockOSPermissionAsker()
        osAsker.requestResult = granted
        let handler = osAsker.makeDeviceCapabilityHandler(execution: MockProductExecution())

        #expect(try await handler(capability) == (granted ? .allowed : .denied))
        #expect(osAsker.requestedCapabilities == [capability.deviceCapabilityType])
    }

    @Test func observesOSRevocation() async throws {
        let osAsker = MockOSPermissionAsker()
        osAsker.statusToReturn = .allowed
        let handler = osAsker.makeDeviceCapabilityHandler(execution: MockProductExecution())
        let first = try await handler(.camera)

        osAsker.statusToReturn = .denied
        let second = try await handler(.camera)

        #expect([first, second] == [.allowed, .denied])
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
