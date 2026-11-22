import ReplayKit
import ImageIO

final class SampleHandler: RPBroadcastSampleHandler {
    private var mailbox: MediaBroadcastMailbox?
    private var generation: Int64 = 0
    private var watchdog: DispatchSourceTimer?
    private let queue = DispatchQueue(label: "host.media.broadcast")
    private var lastFrameTime: Double?

    override func broadcastStarted(withSetupInfo setupInfo: [String: NSObject]?) {
        queue.async { [self] in
            do {
                let mailbox = try MediaBroadcastMailbox(create: false)
                generation = try mailbox.begin()
                self.mailbox = mailbox
                lastFrameTime = nil
                let timer = DispatchSource.makeTimerSource(queue: queue)
                timer.schedule(deadline: .now(), repeating: 1)
                timer.setEventHandler { [weak self] in
                    guard let self, let mailbox = self.mailbox else { return }
                    if (try? mailbox.status(generation: self.generation)) != 2 { self.finish() }
                }
                watchdog = timer
                timer.resume()
            } catch { finish() }
        }
    }

    override func processSampleBuffer(_ sampleBuffer: CMSampleBuffer, with sampleBufferType: RPSampleBufferType) {
        guard sampleBufferType == .video, let image = CMSampleBufferGetImageBuffer(sampleBuffer) else { return }
        queue.sync { [self] in
            guard let mailbox else { return }
            let time = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
            let seconds = CMTimeGetSeconds(time)
            guard seconds.isFinite else { finish(); return }
            if let lastFrameTime, seconds >= lastFrameTime, seconds - lastFrameTime < 1.0 / 15.0 { return }
            lastFrameTime = seconds
            let orientation = (CMGetAttachment(sampleBuffer, key: RPVideoSampleOrientationKey as CFString,
                attachmentModeOut: nil) as? NSNumber)?.uint32Value ?? 1
            let rotation: Int64
            switch CGImagePropertyOrientation(rawValue: orientation) {
            case .right, .rightMirrored: rotation = 90
            case .down, .downMirrored: rotation = 180
            case .left, .leftMirrored: rotation = 270
            default: rotation = 0
            }
            do {
                try mailbox.publish(image, time: time,
                    rotation: rotation, generation: generation)
            } catch { finish() }
        }
    }

    override func broadcastPaused() { queue.async { [self] in mailbox?.stop(generation: generation); finish() } }
    override func broadcastFinished() {
        queue.async { [self] in
            watchdog?.cancel(); watchdog = nil
            mailbox?.stop(generation: generation); mailbox = nil
        }
    }

    private func finish() {
        watchdog?.cancel(); watchdog = nil
        mailbox?.stop(generation: generation); mailbox = nil
        finishBroadcastWithError(NSError(domain: "HostMedia", code: 1,
            userInfo: [NSLocalizedDescriptionKey: "Screen sharing ended."]))
    }
}
