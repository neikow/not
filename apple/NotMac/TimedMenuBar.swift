import AppKit
import NotCore
import SwiftUI

// The Mac's take on the phone's live notification: while a timed habit is out of its usual state
// (paused when it's normally on, running when it's normally off), its timer sits in the menu bar,
// if switched on for that habit on this Mac. Mirrors TimedAlerts.kt and HabitRules.kt.

/// What the menu bar timer counts, chosen per habit on this Mac.
enum LiveCounter: String, CaseIterable {
    /// Up, from when the habit left its usual state.
    case stretch
    /// Up, the time counted toward the goal today.
    case today
    /// Down, to the goal or to the end of the allowance; then up, the time over.
    case left

    var label: String {
        switch self {
        case .stretch: "This Stretch"
        case .today: "Today"
        case .left: "Time Left"
        }
    }
}

extension TimedRule {
    /// Out of its usual state, the habit counts toward the goal: Today and Time Left move then.
    var unusualCounts: Bool { !usualOn != atMostOff }
}

/// A timer for `Text(timerInterval:)`: counting up from `base`, or down to it.
struct Chrono: Equatable {
    let base: Date
    let down: Bool
}

extension TimedLive {
    /// The timer showing `counter` as of `now`. When the running stretch doesn't count toward the
    /// goal, only the stretch moves, so that's what it shows.
    func chrono(_ counter: LiveCounter, now: Date) -> Chrono {
        let counts = tracked && on != atMostOff
        if !counts || counter == .stretch { return Chrono(base: since ?? now, down: false) }
        if counter == .today { return Chrono(base: now.addingTimeInterval(-Double(countedMs) / 1000), down: false) }
        let end = now.addingTimeInterval(Double(leftMs) / 1000)
        // Over the allowance: the time over, counting up.
        return Chrono(base: end, down: leftMs > 0)
    }

    /// Today's goal can't be missed any more: time on reached for "at least" habits; for "at
    /// most", even staying off until midnight would keep within the allowance.
    func secured(now: Date) -> Bool {
        guard tracked else { return false }
        if !atMostOff { return countedMs >= goalMs }
        let dayEnd = Calendar.current.startOfDay(for: now).addingTimeInterval(86_400)
        return countedMs + max(0, Int64(dayEnd.timeIntervalSince(now) * 1000)) <= goalMs
    }
}

/// Per-Mac choices for a habit's menu bar timer.
enum TimedBar {
    static func enabled(_ id: String) -> Bool { UserDefaults.standard.bool(forKey: "menuBar_" + id) }
    static func setEnabled(_ id: String, _ on: Bool) { UserDefaults.standard.set(on, forKey: "menuBar_" + id) }
    static func counter(_ id: String) -> LiveCounter {
        UserDefaults.standard.string(forKey: "counter_" + id).flatMap(LiveCounter.init(rawValue:)) ?? .stretch
    }
    static func setCounter(_ id: String, _ counter: LiveCounter) { UserDefaults.standard.set(counter.rawValue, forKey: "counter_" + id) }
}

extension AppModel {
    /// Timed habits whose timer shows in the menu bar now: switched on here, out of their usual
    /// state, and today's goal not safe yet (then the timer has done its job). Judged against
    /// `clock`, so it updates within half a minute.
    var menuBarTimed: [HabitSummary] {
        _ = timedBarVersion
        return habits.filter { summary in
            guard let rule = summary.habit.timed, TimedBar.enabled(summary.habit.id),
                  let live = summary.timedLive(now: clock, loadedAt: habitsLoadedAt),
                  live.tracked, live.on != rule.usualOn else { return false }
            return !live.secured(now: clock)
        }
    }
}

/// The menu bar item: a timer icon and the first habit's count, ticking by itself.
struct TimedBarLabel: View {
    let model: AppModel

    var body: some View {
        if let summary = model.menuBarTimed.first,
           let live = summary.timedLive(now: .now, loadedAt: model.habitsLoadedAt) {
            let chrono = live.chrono(TimedBar.counter(summary.habit.id), now: .now)
            HStack(spacing: 3) {
                Image(systemName: summary.habit.timed?.usualOn == true ? "pause.circle" : "timer")
                if chrono.down {
                    Text(timerInterval: Date.now...max(chrono.base, .now), countsDown: true)
                } else {
                    Text(chrono.base, style: .timer)
                }
            }
            .monospacedDigit()
        } else {
            Image(systemName: "timer")
        }
    }
}

/// The menu under it: each habit showing, where it stands and its switch.
struct TimedBarMenu: View {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow

    var body: some View {
        ForEach(model.menuBarTimed, id: \.habit.id) { summary in
            if let rule = summary.habit.timed, let live = summary.timedLive(now: .now, loadedAt: model.habitsLoadedAt) {
                Text("\(summary.habit.title) · \(timedStatus(rule, live, now: .now))")
                Button(switchLabel(rule, on: live.on)) { model.switchHabit(summary.habit.id, on: !live.on) }
                Divider()
            }
        }
        Button("Show Habits") {
            model.selection = .habits
            openWindow(id: MainWindow.id)
            NSApp.activate()
        }
    }
}

/// In a timed habit's Info: the menu bar timer and what it counts.
struct TimedBarSettings: View {
    let model: AppModel
    let habit: Habit
    let rule: TimedRule
    @State private var on = false
    @State private var counter = LiveCounter.stretch

    var body: some View {
        Toggle("Show in the menu bar", isOn: Binding(get: { on }, set: { value in
            on = value
            TimedBar.setEnabled(habit.id, value)
            model.timedBarChanged()
        }))
        .help("While \(stateName(rule, on: !rule.usualOn).lowercased()), its timer sits in the menu bar, with a button to \(switchLabel(rule, on: !rule.usualOn).lowercased()). This Mac only.")
        // Only worth a choice when the stretch counts toward the goal; otherwise just it moves.
        if on, rule.unusualCounts {
            Picker("Counts", selection: Binding(get: { counter }, set: { value in
                counter = value
                TimedBar.setCounter(habit.id, value)
                model.timedBarChanged()
            })) {
                ForEach(LiveCounter.allCases, id: \.self) { Text($0.label).tag($0) }
            }
            .pickerStyle(.segmented)
        }
        Color.clear.frame(height: 0)
            .onAppear {
                on = TimedBar.enabled(habit.id)
                counter = TimedBar.counter(habit.id)
            }
            .onChange(of: habit.id) {
                on = TimedBar.enabled(habit.id)
                counter = TimedBar.counter(habit.id)
            }
    }
}
