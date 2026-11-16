import NotCore
import SwiftUI

// Timed habits on the Mac: the row with today's total and the switch, and the basics in the
// inspector. Breaks and the rule are edited on the phone. Mirrors HabitRules.kt on Android.

/// A timed habit as of `now`, from a summary loaded at `loadedAt`: the running stretch keeps
/// counting between reloads.
struct TimedLive {
    let tracked: Bool
    let on: Bool
    let since: Date?
    let onMs: Int64
    let offMs: Int64
    let goalMs: Int64
    let atMostOff: Bool

    /// Time on, or off for "at most" habits.
    var countedMs: Int64 { atMostOff ? offMs : onMs }
    var met: Bool { atMostOff ? countedMs <= goalMs : countedMs >= goalMs }
    /// Time still to go, or allowance left (negative once over).
    var leftMs: Int64 { goalMs - countedMs }
    /// Done so far, or allowance left, 0...1.
    var ringFraction: Double {
        guard goalMs > 0 else { return 0 }
        return min(1, max(0, Double(atMostOff ? leftMs : countedMs) / Double(goalMs)))
    }
}

extension HabitSummary {
    func timedLive(now: Date, loadedAt: Date) -> TimedLive? {
        guard let rule = habit.timed, let state = timed else { return nil }
        let tracked = state.since != nil
        let extra = tracked ? max(0, Int64(now.timeIntervalSince(loadedAt) * 1000)) : 0
        return TimedLive(
            tracked: tracked,
            on: state.on,
            since: state.since.map { Date(timeIntervalSince1970: Double($0) / 1000) },
            onMs: state.onMs + (state.on ? extra : 0),
            offMs: state.offMs + (state.on ? 0 : extra),
            goalMs: Int64(habit.goal) * 60_000,
            atMostOff: rule.atMostOff
        )
    }
}

/// "45m", "1h 05m", "22h".
func durationText(_ ms: Int64) -> String {
    let minutes = max(0, ms) / 60_000
    let (h, m) = (minutes / 60, minutes % 60)
    if h == 0 { return "\(m)m" }
    if m == 0 { return "\(h)h" }
    return String(format: "%dh %02dm", h, m)
}

/// Pause/Resume when it's normally on, else Start/Stop.
func switchLabel(_ rule: TimedRule, on: Bool) -> String {
    rule.usualOn ? (on ? "Pause" : "Resume") : (on ? "Stop" : "Start")
}

func stateName(_ rule: TimedRule, on: Bool) -> String {
    rule.usualOn ? (on ? "On" : "Paused") : (on ? "Running" : "Stopped")
}

/// "Paused 25m · 1h 35m left", "Running 12m · 18m to go".
func timedStatus(_ rule: TimedRule, _ live: TimedLive, now: Date) -> String {
    guard live.tracked, let since = live.since else { return "Not started" }
    let elapsed = Int64(now.timeIntervalSince(since) * 1000)
    let stretch = elapsed < 60_000 ? "just now" : durationText(elapsed)
    let head = live.on != rule.usualOn ? "\(stateName(rule, on: live.on)) \(stretch)" : stateName(rule, on: live.on)
    let tail = if live.atMostOff {
        live.leftMs >= 0 ? "\(durationText(live.leftMs)) left" : "\(durationText(-live.leftMs)) over"
    } else {
        live.met ? "\(durationText(live.countedMs)) today ✓" : "\(durationText(live.leftMs)) to go"
    }
    return "\(head) · \(tail)"
}

/// Time toward the goal fills the ring; an allowance empties it, amber under a quarter, red once over.
struct TimedRingView: View {
    let live: TimedLive?
    var size: CGFloat = 24

    var body: some View {
        let fraction = live?.ringFraction ?? 0
        let over = live.map { $0.atMostOff && $0.leftMs < 0 } ?? false
        let met = live.map { !$0.atMostOff && $0.met } ?? false
        let color: Color = over ? .red : (live?.atMostOff == true && fraction < 0.25) ? .orange : .accentColor
        ZStack {
            if met {
                Circle().fill(.tint)
                Image(systemName: "checkmark").font(.system(size: size * 0.45, weight: .bold)).foregroundStyle(.white)
            } else {
                Circle().strokeBorder(.secondary.opacity(0.3), lineWidth: 2.5)
                Circle()
                    .trim(from: 0, to: over ? 1 : fraction)
                    .stroke(color, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                    .padding(1.5)
            }
        }
        .frame(width: size, height: size)
        .animation(.snappy, value: fraction)
    }
}

/// Ring, title, where it stands (counting live) and the switch.
struct TimedHabitRow: View {
    let summary: HabitSummary
    let model: AppModel

    var body: some View {
        let habit = summary.habit
        TimelineView(.periodic(from: .now, by: 1)) { context in
            if let rule = habit.timed, let live = summary.timedLive(now: context.date, loadedAt: model.habitsLoadedAt) {
                HStack(spacing: 12) {
                    TimedRingView(live: live)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(habit.title).foregroundStyle(summary.due ? .primary : .secondary)
                        HStack(spacing: 8) {
                            Text(timedStatus(rule, live, now: context.date))
                                .foregroundStyle(live.atMostOff && live.leftMs < 0 ? AnyShapeStyle(.red)
                                    : live.tracked && live.on != rule.usualOn ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                            if summary.streak > 0 {
                                Label("\(summary.streak)", systemImage: "flame.fill").foregroundStyle(.orange)
                            }
                        }
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 8)
                    TimedSwitchButton(rule: rule, live: live) { model.switchHabit(habit.id, on: !live.on) }
                }
                .padding(.vertical, 4)
            }
        }
    }
}

/// Resume or Start in the accent color; Pause or Stop quiet.
struct TimedSwitchButton: View {
    let rule: TimedRule
    let live: TimedLive
    let action: () -> Void

    var body: some View {
        let leavingUsual = live.tracked && live.on == rule.usualOn
        Button(action: action) {
            Label(switchLabel(rule, on: live.on), systemImage: live.on ? "pause.fill" : "play.fill")
        }
        .buttonStyle(.borderedProminent)
        .tint(leavingUsual ? .secondary : .accentColor)
        .controlSize(.regular)
    }
}

/// The inspector's Today and Schedule for a timed habit.
struct TimedDetailSections: View {
    let model: AppModel
    let summary: HabitSummary

    var body: some View {
        let habit = summary.habit
        if let rule = habit.timed {
            Section("Today") {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    if let live = summary.timedLive(now: context.date, loadedAt: model.habitsLoadedAt) {
                        HStack(spacing: 14) {
                            TimedRingView(live: live, size: 36)
                            VStack(alignment: .leading) {
                                Text(timedStatus(rule, live, now: context.date)).font(.title3.monospacedDigit())
                                Text("\(durationText(live.onMs)) \(stateName(rule, on: true).lowercased()) · \(durationText(live.offMs)) \(stateName(rule, on: false).lowercased()) today")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                            Spacer()
                            TimedSwitchButton(rule: rule, live: live) { model.switchHabit(habit.id, on: !live.on) }
                        }
                    }
                }
            }
            Section("Goal") {
                Stepper(value: Binding(
                    get: { Int(habit.goal) },
                    set: { model.updateHabit(Habit(id: habit.id, title: habit.title, goal: UInt32($0), days: habit.days, period: habit.period, timed: rule)) }
                ), in: 5...(24 * 60), step: 15) {
                    Text(rule.atMostOff ? "At most \(durationText(Int64(habit.goal) * 60_000)) \(stateName(rule, on: false).lowercased()) a day"
                        : "At least \(durationText(Int64(habit.goal) * 60_000)) \(stateName(rule, on: true).lowercased()) a day")
                }
                WeekdayPicker(days: Binding(
                    get: { habit.days },
                    set: { model.updateHabit(Habit(id: habit.id, title: habit.title, goal: habit.goal, days: $0, period: habit.period, timed: rule)) }
                ))
                Text("Fix breaks and change the rule in the app on your phone.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
    }
}
