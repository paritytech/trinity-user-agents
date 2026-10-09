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

        #expect(await sut.ensurePermissions(for: .audio) == scenario.2)
    }

    @Test("Refused prompt fails audio call permissions")
    func ensurePermissionsRefused() async {
        let sut = makeSut(
            permission: .undetermined,
            appState: .active,
            requester: RecordPermissionRequesterSpy(grants: false)
        )

        #expect(await sut.ensurePermissions(for: .audio) == false)
    }
}

private extension CallPermissionsServiceTests {
    func makeSut(
        permission: AVAudioApplication.recordPermission,
        appState: UIApplication.State,
        requester: RecordPermissionRequesterSpy
    ) -> CallPermissionsService {
        CallPermissionsService(
            applicationStateProvider: { appState },
            recordPermissionProvider: StubRecordPermissionProvider(recordPermission: permission),
            recordPermissionRequester: requester
        )
    }
}
