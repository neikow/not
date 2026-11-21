import AppKit
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
    static let timedKey = "remindTimed"

    nonisolated private static let category = "task"
    nonisolated private static let done = "done"
    nonisolated private static let snooze = "snooze"
    /// Timed habit nudges: one category per button title, since titles are fixed per category.
    nonisolated private static let timedResume = "timedResume"
    nonisolated private static let timedStop = "timedStop"
    nonisolated private static let switchBack = "switchBack"
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
            Self.timedKey: false,
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
            UNNotificationCategory(identifier: Self.timedResume, actions: [UNNotificationAction(identifier: Self.switchBack, title: "Resume")], intentIdentifiers: []),
            UNNotificationCategory(identifier: Self.timedStop, actions: [UNNotificationAction(identifier: Self.switchBack, title: "Stop")], intentIdentifiers: []),
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
        let wantsTimed = defaults.bool(forKey: Self.timedKey)
        center.removeAllPendingNotificationRequests()
        // A stretch that ended has nothing left to say.
        if wantsTimed { clearEndedNudges(model) }
        guard wantsTasks || wantsDigest || wantsHabits || wantsTimed else { return }
        guard (try? await center.requestAuthorization(options: [.alert, .sound])) == true else { return }

        let now = Date()
        var requests: [UNNotificationRequest] = []
        if wantsTasks {
            let horizon = now.addingTimeInterval(7 * 86_400)
            for task in model.reminderTasks() {
                guard let at = dueDate(task), at > now, at < horizon else { continue }
                let content = UNMutableNotificationContent()
                content.title = task.title
                content.body = [timeLabel(task), task.tags.isEmpty ? nil : task.tags.map { "#" + $0 }.joined(separator: " ")].compactMap { $0 }.joined(separator: " · ")
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
        if wantsTimed {
            requests += timedNudges(model, now: now)
        }
        for request in requests {
            try? await center.add(request)
        }
    }

    /// A timed habit out of its usual state, once its stretch reaches the habit's reminder delay:
    /// "Retainer paused for 30m", with a button to switch back. One per stretch (its id has the
    /// stretch's start), and only ahead of now, so nothing repeats.
    private func timedNudges(_ model: AppModel, now: Date) -> [UNNotificationRequest] {
        model.habits.compactMap { summary in
            guard let rule = summary.habit.timed, rule.nudgeMinutes > 0, let state = summary.timed,
                  let since = state.since, state.on != rule.usualOn else { return nil }
            let at = Date(timeIntervalSince1970: Double(since) / 1000 + Double(rule.nudgeMinutes) * 60)
            guard at > now else { return nil }
            let content = UNMutableNotificationContent()
            let stretch = durationText(Int64(rule.nudgeMinutes) * 60_000)
            content.title = rule.usualOn ? "\(summary.habit.title) paused for \(stretch)" : "\(summary.habit.title) running for \(stretch)"
            content.body = rule.usualOn ? "Resume it when it's back on." : "Stop it if you're done."
            content.sound = .default
            content.categoryIdentifier = rule.usualOn ? Self.timedResume : Self.timedStop
            content.userInfo = ["habit": summary.habit.id, "on": rule.usualOn]
            return UNNotificationRequest(
                identifier: "timed-\(summary.habit.id)",
                content: content,
                trigger: UNTimeIntervalNotificationTrigger(timeInterval: at.timeIntervalSince(now), repeats: false)
            )
        }
    }

    /// Takes a delivered nudge away once its habit is back in its usual state.
    private func clearEndedNudges(_ model: AppModel) {
        let ended = model.habits.compactMap { summary -> String? in
            guard let rule = summary.habit.timed, summary.timed?.on == rule.usualOn else { return nil }
            return "timed-\(summary.habit.id)"
        }
        center.removeDeliveredNotifications(withIdentifiers: ended)
    }

    private func request(_ id: String, _ content: UNNotificationContent, at date: Date) -> UNNotificationRequest {
        let parts = Calendar.current.dateComponents([.year, .month, .day, .hour, .minute], from: date)
        return UNNotificationRequest(identifier: id, content: content, trigger: UNCalendarNotificationTrigger(dateMatching: parts, repeats: false))
    }

    private func dueDate(_ task: TaskItem) -> Date? {
        guard let due = task.start, let minutes = task.time,
              let day = AppModel.dayFormatter.date(from: due) else { return nil }
        return Calendar.current.date(byAdding: .minute, value: Int(minutes), to: day)
    }

    private func timeLabel(_ task: TaskItem) -> String? {
        task.time.map { String(format: "%d:%02d", $0 / 60, $0 % 60) }
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
        if let habit = content.userInfo["habit"] as? String {
            let on = content.userInfo["on"] as? Bool ?? true
            let switching = response.actionIdentifier == Self.switchBack
            await MainActor.run {
                if switching {
                    model?.switchHabit(habit, on: on)
                } else {
                    model?.openHabit(habit)
                    NSApp.activate()
                }
            }
            return
        }
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
    @AppStorage(Reminders.timedKey) private var timed = false

    var body: some View {
        Section {
            Toggle("When a task with a time is due", isOn: $tasks)
            Toggle("Morning summary of what's due", isOn: $digest)
            if digest { TimeField(label: "Summary at", minutes: $digestAt) }
            Toggle("Evening nudge for habits left", isOn: $habits)
            if habits { TimeField(label: "Nudge at", minutes: $habitsAt) }
            Toggle("When a timed habit is paused or running too long", isOn: $timed)
        } header: {
            Text("Reminders")
        } footer: {
            Text("On this Mac only. The summary and the habit reminders are off by default here, since the phone sends them too. A timed habit's delay is set in its Info.")
                .foregroundStyle(.secondary)
        }
        .onChange(of: [tasks, digest, habits, timed]) { model.reminders.reschedule() }
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
