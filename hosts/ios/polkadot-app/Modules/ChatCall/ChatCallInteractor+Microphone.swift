import Foundation

extension ChatCallInteractor {
    func setMuted(_ isMuted: Bool, notifiesCallKit: Bool) async {
        if notifiesCallKit {
            callKitManager.requestMutedFromApp(isMuted)
        }

        let result = await callEngine.setMuted(isMuted)
        callKitManager.confirmMutedState(isSuccessful: isMuted == result)

        await presenter?.didUpdateMuteState(result)
    }

    func startMutedIfNeeded(for microphoneAccess: CallMicrophoneAccess) async {
        guard microphoneAccess != .granted else {
            return
        }

        logger.warning("Microphone not available (\(microphoneAccess)), answering muted")
        await setMuted(true, notifiesCallKit: true)

        if microphoneAccess == .denied {
            await presenter?.didRequireMicrophoneAccess()
        }
    }

    func unmute(notifiesCallKit: Bool) async {
        guard !permissionsService.isMicrophoneGranted else {
            await unmuteWithGrantedMicrophone(notifiesCallKit: notifiesCallKit)
            return
        }

        if !notifiesCallKit {
            callKitManager.confirmMutedState(isSuccessful: false)
        }

        await presenter?.didUpdateMuteState(true)

        let microphoneAccess = await permissionsService.resolveMicrophoneAccess(prompting: .always)

        guard !isEnding else {
            return
        }

        switch microphoneAccess {
        case .granted:
            await unmuteWithGrantedMicrophone(notifiesCallKit: true)
        case .denied:
            await presenter?.didRequireMicrophoneAccess()
        case .refused,
             .deferred:
            logger.warning("Microphone access not granted, staying muted")
        }
    }
}

private extension ChatCallInteractor {
    func unmuteWithGrantedMicrophone(notifiesCallKit: Bool) async {
        audioSessionManager.enableAudioIfPermitted()
        await setMuted(false, notifiesCallKit: notifiesCallKit)
    }
}
