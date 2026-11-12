#if targetEnvironment(simulator)
    import CallKit
    import AsyncExtensions

    /// The simulator has no system call UI, so iOS ends every CallKit call right after it starts
    /// ("there wont be a UI to host the call"). Calls run without CallKit here and only track
    /// the active call for signaling retention.
    final class SimulatorCallKitManager {
        private let activeCallDataSubject = AsyncCurrentValueSubject<VoIPCallKitData?>(nil)

        private func startCall(with input: VoIPCallKitInput) {
            activeCallDataSubject.send(
                VoIPCallKitData(
                    uuid: UUID(),
                    status: .reported(input),
                    pendingAnswerOrEndActionSource: nil,
                    pendingMutedActionSource: nil
                )
            )
        }
    }

    extension SimulatorCallKitManager: VoIPCallKitManaging {
        func reportIncomingCall(fromPushPayload _: [AnyHashable: Any], completion: @escaping () -> Void) {
            completion()
        }

        func startOutgoingCall(with input: VoIPCallKitInput) {
            startCall(with: input)
        }

        func markCallConnecting(for _: CallRole) {}

        func markCallConnected(for _: CallRole) {}

        func markCallDisconnected(with _: CXCallEndedReason) {
            activeCallDataSubject.send(nil)
        }

        func answerFromAppOrEnsureStarted(with input: VoIPCallKitInput) {
            startCall(with: input)
        }

        func endFromApp() {
            activeCallDataSubject.send(nil)
        }

        func requestMutedFromApp(_: Bool) {}

        func confirmMutedState(isSuccessful _: Bool) {}

        func observeActiveCallData() -> AnyAsyncSequence<VoIPCallKitData?> {
            activeCallDataSubject.eraseToAnyAsyncSequence()
        }

        func observeHasPendingAnswer() -> AnyAsyncSequence<Bool> {
            AsyncJustSequence(false).eraseToAnyAsyncSequence()
        }

        func observeHasPendingEnd() -> AnyAsyncSequence<Bool> {
            AsyncJustSequence(false).eraseToAnyAsyncSequence()
        }

        func observeMutedAction() -> AnyAsyncSequence<Bool?> {
            AsyncJustSequence(nil).eraseToAnyAsyncSequence()
        }
    }
#endif
