import Foundation
import Foundation_iOS

final class AlarmScheduler {
    let closure: () -> Void
    let queue: DispatchQueue
    let timeInterval: TimeInterval

    init(
        timeInterval: TimeInterval,
        queue: DispatchQueue,
        closure: @escaping () -> Void
    ) {
        self.timeInterval = timeInterval
        self.queue = queue
        self.closure = closure
    }

    var scheduler: SchedulerProtocol?

    func start() {
        stop()

        scheduler = Scheduler(with: self, callbackQueue: nil)
        scheduler?.notifyAfter(timeInterval)
    }

    func stop() {
        scheduler?.cancel()
        scheduler = nil
    }
}

extension AlarmScheduler: SchedulerDelegate {
    func didTrigger(scheduler _: SchedulerProtocol) {
        queue.async { [weak self] in
            self?.closure()
        }
    }
}

extension AlarmScheduler {
    static func scheduleAlarm(
        after timeInterval: TimeInterval,
        notifyingIn queue: DispatchQueue,
        closure: @escaping () -> Void
    ) -> AlarmScheduler {
        let alarm = AlarmScheduler(timeInterval: timeInterval, queue: queue, closure: closure)
        alarm.start()
        return alarm
    }
}
