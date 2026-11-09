import AppKit
import Observation
import UserNotifications

/// Countdown between sets. When it runs out: a sound, a notification if the app is in the
/// background, and a Dock bounce.
@MainActor
@Observable
final class RestTimer {
    /// When the running countdown ends.
    private(set) var endsAt: Date?
    /// Seconds, remembered across launches.
    var duration: Int = UserDefaults.standard.object(forKey: RestTimer.durationKey) as? Int ?? 90 {
        didSet { UserDefaults.standard.set(duration, forKey: Self.durationKey) }
    }
    /// Starts by itself after a set is logged.
    var startsAfterSet: Bool = UserDefaults.standard.object(forKey: RestTimer.autoKey) as? Bool ?? true {
        didSet { UserDefaults.standard.set(startsAfterSet, forKey: Self.autoKey) }
    }

    static let presets = [60, 90, 120, 180, 240]

    @ObservationIgnored private var ticker: Task<Void, Never>?
    @ObservationIgnored private var askedForNotifications = false
    private static let durationKey = "restDuration"
    private static let autoKey = "restAfterSet"
    private static let notificationID = "rest-timer"

    var isRunning: Bool { endsAt != nil }

    func start(seconds: Int? = nil) {
        if let seconds { duration = seconds }
        run(until: .now.addingTimeInterval(TimeInterval(duration)))
    }

    /// Adds (or with a negative value takes off) time from the running countdown.
    func extend(by seconds: Int) {
        guard let endsAt else { return }
        let ends = endsAt.addingTimeInterval(TimeInterval(seconds))
        if ends <= .now { stop() } else { run(until: ends) }
    }

    private func run(until ends: Date) {
        endsAt = ends
        ticker?.cancel()
        ticker = Task { [weak self] in
            try? await Task.sleep(for: .seconds(ends.timeIntervalSinceNow))
            guard !Task.isCancelled else { return }
            self?.finish()
        }
        scheduleNotification(at: ends)
    }

    func stop() {
        ticker?.cancel()
        ticker = nil
        endsAt = nil
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: [Self.notificationID])
    }

    private func finish() {
        endsAt = nil
        ticker = nil
        NSSound(named: "Glass")?.play()
        if !NSApp.isActive { NSApp.requestUserAttention(.informationalRequest) }
    }

    /// The notification shows only when the app is in the background; in front, the sound is enough.
    private func scheduleNotification(at date: Date) {
        let center = UNUserNotificationCenter.current()
        center.removePendingNotificationRequests(withIdentifiers: [Self.notificationID])
        let content = UNMutableNotificationContent()
        content.title = "Rest is over"
        content.body = "Time for the next set."
        let trigger = UNTimeIntervalNotificationTrigger(timeInterval: max(1, date.timeIntervalSinceNow), repeats: false)
        let request = UNNotificationRequest(identifier: Self.notificationID, content: content, trigger: trigger)
        let ask = !askedForNotifications
        askedForNotifications = true
        Task {
            if ask { _ = try? await center.requestAuthorization(options: [.alert]) }
            try? await center.add(request)
        }
    }

    static func format(_ seconds: Int) -> String {
        String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}
