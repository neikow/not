import Foundation
import NotCore
import SwiftUI
import UserNotifications

/// Local notifications for tasks with a time, a morning summary and an evening habit nudge. Kept
/// per device (in UserDefaults): the phone can ring while the Mac stays quiet. [reschedule] replaces
/// everything pending, so it runs after every change.
@MainActor
final class Reminders: NSObject, UNUserNotificationCenterDelegate {
    static let tasksKey = "remindTasks"
    static let digestKey = "remindDigest"
    static let digestAtKey = "remindDigestAt"
    static let habitsKey = "remindHabits"
    static let habitsAtKey = "remindHabitsAt"

    nonisolated private static let category = "task"
    nonisolated private static let done = "done"
    nonisolated private static let snooze = "snooze"
    /// macOS keeps at most 64 pending requests per app.
    private static let maxTasks = 50

    private weak var model: AppModel?
    private let center = UNUserNotificationCenter.current()
    private var pending: Task<Void, Never>?

    init(model: AppModel) {
        self.model = model
        super.init()
        UserDefaults.standard.register(defaults: [
            Self.tasksKey: true,
            // The phone usually has these; off here so they don't arrive twice.
            Self.digestKey: false,
            Self.digestAtKey: 9 * 60,
            Self.habitsKey: false,
            Self.habitsAtKey: 20 * 60,
        ])
        center.delegate = self
        center.setNotificationCategories([
            UNNotificationCategory(
                identifier: Self.category,
                actions: [
                    UNNotificationAction(identifier: Self.done, title: "Done"),
                    UNNotificationAction(identifier: Self.snooze, title: "In 15 Minutes"),
                ],
                intentIdentifiers: []
            ),
        ])
    }

    /// Coalesces bursts (typing, a big pull) into one pass.
    func reschedule() {
        pending?.cancel()
        pending = Task { [weak self] in
            try? await Task.sleep(for: .seconds(1))
            guard !Task.isCancelled else { return }
            await self?.apply()
        }
    }

    private func apply() async {
        guard let model else { return }
        let defaults = UserDefaults.standard
        let wantsTasks = defaults.bool(forKey: Self.tasksKey)
        let wantsDigest = defaults.bool(forKey: Self.digestKey)
        let wantsHabits = defaults.bool(forKey: Self.habitsKey)
        center.removeAllPendingNotificationRequests()
        guard wantsTasks || wantsDigest || wantsHabits else { return }
        guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }

        let now = Date()
        var requests: [UNNotificationRequest] = []
        if wantsTasks {
            let horizon = now.addingTimeInterval(7 * 86_400)
            for task in model.reminderTasks() {
                guard let at = dueDate(task), at > now, at < horizon else { continue }
                let content = UNMutableNotificationContent()
                content.title = task.title
                content.body = [timeLabel(task), model.listTitle(task.listId)].compactMap { $0 }.joined(separator: " · ")
                content.sound = .default
                content.categoryIdentifier = Self.category
                content.userInfo = ["id": task.id]
                requests.append(request("task-\(task.id)", content, at: at))
                if requests.count >= Self.maxTasks { break }
            }
        }
        if wantsDigest {
            let at = next(minutes: defaults.integer(forKey: Self.digestAtKey), after: now)
            // As of when it fires: due by that day, from what's known now.
            let due = model.reminderDue(on: AppModel.dayString(at))
            if !due.isEmpty {
                let content = UNMutableNotificationContent()
                content.title = due.count == 1 ? "1 task today" : "\(due.count) tasks today"
                content.body = due.map(\.title).joined(separator: ", ")
                requests.append(request("digest", content, at: at))
            }
        }
        if wantsHabits {
            let at = next(minutes: defaults.integer(forKey: Self.habitsAtKey), after: now)
            let left = model.reminderHabitsLeft(on: AppModel.dayString(at))
            if !left.isEmpty {
                let content = UNMutableNotificationContent()
                content.title = left.count == 1 ? "1 habit left today" : "\(left.count) habits left today"
                content.body = left.joined(separator: ", ")
                requests.append(request("habits", content, at: at))
            }
        }
        for request in requests {
            try? await center.add(request)
        }
    }

    private func request(_ id: String, _ content: UNNotificationContent, at date: Date) -> UNNotificationRequest {
        let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: date)
        return UNNotificationRequest(identifier: id, content: content, trigger: UNCalendarNotificationTrigger(dateMatching: parts, repeats: false))
    }

    private func dueDate(_ task: TaskItem) -> Date? {
        guard let due = task.due, let minutes = task.dueTime,
              let day = AppModel.dayFormatter.date(from: due) else { return nil }
        return Calendar.current.date(byAdding: .minute, value: Int(minutes), to: day)
    }

    private func timeLabel(_ task: TaskItem) -> String? {
        task.dueTime.map { String(format: "%d:%02d", $0 / 60, $0 % 60) }
    }

    private func next(minutes: Int, after now: Date) -> Date {
        let today = Calendar.current.startOfDay(for: now).addingTimeInterval(TimeInterval(minutes * 60))
        return today > now ? today : Calendar.current.date(byAdding: .day, value: 1, to: today)!
    }

    // ---- delegate ----

    /// Shown even while the app is in front: a reminder is the point.
    nonisolated func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        [.banner, .sound]
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive response: UNNotificationResponse) async {
        let content = response.notification.request.content
        guard let id = content.userInfo["id"] as? String else { return }
        switch response.actionIdentifier {
        case Self.snooze:
            let again = UNMutableNotificationContent()
            again.title = content.title
            again.body = content.body
            again.sound = .default
            again.categoryIdentifier = Self.category
            again.userInfo = ["id": id]
            let trigger = UNTimeIntervalNotificationTrigger(timeInterval: 15 * 60, repeats: false)
            try? await center.add(UNNotificationRequest(identifier: "snooze-\(id)", content: again, trigger: trigger))
        case Self.done:
            await MainActor.run { model?.complete(taskID: id) }
        default:
            await MainActor.run { model?.reveal(taskID: id) }
        }
    }
}

/// Settings → General: what this Mac reminds about.
struct ReminderSettings: View {
    let model: AppModel
    @AppStorage(Reminders.tasksKey) private var tasks = true
    @AppStorage(Reminders.digestKey) private var digest = false
    @AppStorage(Reminders.digestAtKey) private var digestAt = 9 * 60
    @AppStorage(Reminders.habitsKey) private var habits = false
    @AppStorage(Reminders.habitsAtKey) private var habitsAt = 20 * 60

    var body: some View {
        Section {
            Toggle("When a task with a time is due", isOn: $tasks)
            Toggle("Morning summary of what's due", isOn: $digest)
            if digest { TimeField(label: "Summary at", minutes: $digestAt) }
            Toggle("Evening nudge for habits left", isOn: $habits)
            if habits { TimeField(label: "Nudge at", minutes: $habitsAt) }
        } header: {
            Text("Reminders")
        } footer: {
            Text("On this Mac only. The summary and nudge are off by default here, since the phone sends them too.")
                .foregroundStyle(.secondary)
        }
        .onChange(of: [tasks, digest, habits]) { model.reminders.reschedule() }
        .onChange(of: [digestAt, habitsAt]) { model.reminders.reschedule() }
    }
}

private struct TimeField: View {
    let label: String
    @Binding var minutes: Int

    var body: some View {
        DatePicker(label, selection: Binding(
            get: { Calendar.current.startOfDay(for: .now).addingTimeInterval(TimeInterval(minutes * 60)) },
            set: {
                let parts = Calendar.current.dateComponents([.hour, .minute], from: $0)
                minutes = (parts.hour ?? 0) * 60 + (parts.minute ?? 0)
            }
        ), displayedComponents: .hourAndMinute)
    }
}
