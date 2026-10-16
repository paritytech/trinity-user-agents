@testable import polkadot_app
import AVFoundation
import Foundation
import Testing
import UIKit

struct CallPermissionsServiceTests {
    @Test(
        "Settled permission is returned without prompting",
        arguments: [
            (AVAudioApplication.recordPermission.granted, CallMicrophoneAccess.granted),
            (.denied, .denied)
        ],
        [MicrophonePromptPolicy.whenActive, .always]
    )
    func settledPermission(
        status: (AVAudioApplication.recordPermission, CallMicrophoneAccess),
        policy: MicrophonePromptPolicy
    ) async {
        let requester = RecordPermissionRequesterSpy(grants: true)
        let sut = makeSut(permission: status.0, appState: .background, requester: requester)

        let access = await sut.resolveMicrophoneAccess(prompting: policy)

        #expect(access == status.1)
        #expect(requester.requestCount == 0)
    }

    @Test("Answering in background defers instead of awaiting a prompt that can't appear")
    func whenActiveInBackgroundDefers() async {
        let requester = RecordPermissionRequesterSpy(grants: true)
        let sut = makeSut(permission: .undetermined, appState: .background, requester: requester)

        let access = await sut.resolveMicrophoneAccess(prompting: .whenActive)

        #expect(access == .deferred)
        #expect(requester.requestCount == 0)
    }

    @Test(
        "Active app prompts and maps the answer",
        arguments: [(true, CallMicrophoneAccess.granted), (false, .refused)]
    )
    func whenActiveInForegroundPrompts(answer: (Bool, CallMicrophoneAccess)) async {
        let requester = RecordPermissionRequesterSpy(grants: answer.0)
        let sut = makeSut(permission: .undetermined, appState: .active, requester: requester)

        let access = await sut.resolveMicrophoneAccess(prompting: .whenActive)

        #expect(access == answer.1)
        #expect(requester.requestCount == 1)
    }

    @Test("Unmuting during a call prompts even in background")
    func alwaysInBackgroundPrompts() async {
        let requester = RecordPermissionRequesterSpy(grants: true)
        let sut = makeSut(permission: .undetermined, appState: .background, requester: requester)

        let access = await sut.resolveMicrophoneAccess(prompting: .always)

        #expect(access == .granted)
        #expect(requester.requestCount == 1)
    }

    @Test(
        "Audio call permissions hold only when the microphone is granted",
        arguments: [
            (AVAudioApplication.recordPermission.granted, UIApplication.State.background, true),
            (.denied, .active, false),
            (.undetermined, .background, false)
        ]
    )
    func ensurePermissions(
        scenario: (AVAudioApplication.recordPermission, UIApplication.State, Bool)
    ) async {
        let sut = makeSut(
            permission: scenario.0,
            appState: scenario.1,
            requester: RecordPermissionRequesterSpy(grants: true)
        )

        #expect(await sut.ensurePermissions() == scenario.2)
    }

    @Test(
        "Settled camera status is reported without prompting",
        arguments: [
            (CameraPermissionResult.authorized, true),
            (.denied, false)
        ]
    )
    func settledCameraStatus(scenario: (CameraPermissionResult, Bool)) async {
        let camera = CameraPermissionServiceSpy(status: scenario.0)
        let sut = makeCameraSut(camera: camera, appState: .active)

        #expect(await sut.ensureCameraAccess() == scenario.1)
        #expect(sut.isCameraGranted == scenario.1)
        #expect(sut.isCameraDenied == !scenario.1)
        #expect(camera.requestCount == 0)
    }

    @Test("Camera access in background defers instead of awaiting a prompt that can't appear")
    func cameraInBackgroundDefers() async {
        let camera = CameraPermissionServiceSpy(status: .notDetermined, promptResult: .authorized)
        let sut = makeCameraSut(camera: camera, appState: .background)

        #expect(await sut.ensureCameraAccess() == false)
        #expect(sut.isCameraDenied == false)
        #expect(camera.requestCount == 0)
    }

    @Test(
        "Active app prompts for the camera and maps the answer",
        arguments: [(CameraPermissionResult.authorized, true), (.denied, false)]
    )
    func cameraInForegroundPrompts(answer: (CameraPermissionResult, Bool)) async {
        let camera = CameraPermissionServiceSpy(status: .notDetermined, promptResult: answer.0)
        let sut = makeCameraSut(camera: camera, appState: .active)

        #expect(await sut.ensureCameraAccess() == answer.1)
        #expect(camera.requestCount == 1)
    }

    @Test("Refused prompt fails audio call permissions")
    func ensurePermissionsRefused() async {
        let sut = makeSut(
            permission: .undetermined,
            appState: .active,
            requester: RecordPermissionRequesterSpy(grants: false)
        )

        #expect(await sut.ensurePermissions() == false)
    }
}

private extension CallPermissionsServiceTests {
    func makeSut(
        permission: AVAudioApplication.recordPermission,
        appState: UIApplication.State,
        requester: RecordPermissionRequesterSpy,
        cameraPermissionService: CameraPermissionServicing = CameraPermissionServiceSpy(status: .denied)
    ) -> CallPermissionsService {
        CallPermissionsService(
            applicationStateProvider: { appState },
            recordPermissionProvider: StubRecordPermissionProvider(recordPermission: permission),
            recordPermissionRequester: requester,
            cameraPermissionService: cameraPermissionService
        )
    }

    func makeCameraSut(
        camera: CameraPermissionServiceSpy,
        appState: UIApplication.State
    ) -> CallPermissionsService {
        makeSut(
            permission: .granted,
            appState: appState,
            requester: RecordPermissionRequesterSpy(grants: true),
            cameraPermissionService: camera
        )
    }
}
