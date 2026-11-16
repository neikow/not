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

/// The inspector for a timed habit: today (switch, backdating), the day's stretches to fix, the
/// rule and goal, and the weekdays.
struct TimedDetailSections: View {
    let model: AppModel
    let summary: HabitSummary
    @State private var editing: StretchDraft?

    var body: some View {
        let habit = summary.habit
        if let rule = habit.timed {
            Section("Today") {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    if let live = summary.timedLive(now: context.date, loadedAt: model.habitsLoadedAt) {
                        VStack(alignment: .leading, spacing: 10) {
                        HStack(spacing: 14) {
                            TimedRingView(live: live, size: 36)
                            VStack(alignment: .leading) {
                                Text(timedStatus(rule, live, now: context.date)).font(.title3.monospacedDigit())
                                Text("\(durationText(live.onMs)) \(stateName(rule, on: true).lowercased()) · \(durationText(live.offMs)) \(stateName(rule, on: false).lowercased()) today")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        // The inspector is narrow: the buttons get their own line.
                        HStack {
                            TimedSwitchButton(rule: rule, live: live) { model.switchHabit(habit.id, on: !live.on) }
                            Menu {
                                ForEach([5, 15, 30, 60], id: \.self) { minutes in
                                    Button("\(stateName(rule, on: !live.on)) \(minutes) Minutes Ago") {
                                        model.setHabitState(habit.id, on: !live.on, from: context.date.addingTimeInterval(Double(-minutes * 60)))
                                    }
                                }
                            } label: {
                                Image(systemName: "clock.arrow.circlepath")
                            }
                            .menuStyle(.borderlessButton)
                            .fixedSize()
                            .help("It happened earlier")
                            Spacer()
                        }
                        }
                    }
                }
            }

            Section("Timeline") {
                let segments = model.habitTimeline(habit.id)
                DayBarView(segments: segments, rule: rule)
                if segments.isEmpty {
                    Text("Nothing tracked today.").foregroundStyle(.secondary)
                }
                ForEach(Array(segments.enumerated()), id: \.offset) { i, seg in
                    let open = i == segments.count - 1
                    Button {
                        editing = StretchDraft(segment: seg, prev: i > 0 ? segments[i - 1] : nil, next: i + 1 < segments.count ? segments[i + 1] : nil, open: open)
                    } label: {
                        HStack {
                            Circle().fill(seg.on ? Color.accentColor : (seg.on == rule.usualOn ? Color.secondary : Color.orange)).frame(width: 8, height: 8)
                            Text("\(clock(seg.start)) – \(open ? "now" : clock(seg.end))")
                            Spacer()
                            Text(stateName(rule, on: seg.on)).foregroundStyle(.secondary)
                            Text(durationText(seg.end - seg.start)).monospacedDigit().frame(width: 64, alignment: .trailing)
                        }
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                Button(rule.usualOn ? "Add a Break…" : "Add a Session…") {
                    editing = StretchDraft(segment: nil, prev: nil, next: nil, open: false)
                }
            }
            .popover(item: $editing) { draft in
                StretchEditor(draft: draft, rule: rule) { on, start, end in
                    edit(habit.id, draft, on: on, start: start, end: end)
                    editing = nil
                } onCancel: { editing = nil }
            }

            Section("Goal") {
                Picker("Normally", selection: Binding(get: { rule.usualOn }, set: { save(rule: TimedRule(atMostOff: rule.atMostOff, usualOn: $0, nudgeMinutes: rule.nudgeMinutes)) })) {
                    Text("On").tag(true)
                    Text("Off").tag(false)
                }
                .pickerStyle(.segmented)
                Picker("Counts", selection: Binding(get: { rule.atMostOff }, set: { save(rule: TimedRule(atMostOff: $0, usualOn: rule.usualOn, nudgeMinutes: rule.nudgeMinutes)) })) {
                    Text("At least \(stateName(rule, on: true).lowercased())").tag(false)
                    Text("At most \(stateName(rule, on: false).lowercased())").tag(true)
                }
                .pickerStyle(.segmented)
                Stepper(value: Binding(get: { Int(habit.goal) }, set: { save(minutes: UInt32($0)) }), in: 5...(24 * 60), step: 15) {
                    Text("\(rule.atMostOff ? "At most" : "At least") \(durationText(Int64(habit.goal) * 60_000)) a day")
                }
                Stepper(value: Binding(get: { Int(rule.nudgeMinutes) }, set: { save(rule: TimedRule(atMostOff: rule.atMostOff, usualOn: rule.usualOn, nudgeMinutes: UInt32($0))) }), in: 0...180, step: 15) {
                    Text(rule.nudgeMinutes == 0 ? "No reminder" : "Remind after \(durationText(Int64(rule.nudgeMinutes) * 60_000)) \(stateName(rule, on: !rule.usualOn).lowercased())")
                }
                Text("Reminders and the lock-screen notification are on your phone.")
                    .font(.caption).foregroundStyle(.secondary)
                WeekdayPicker(days: Binding(get: { habit.days }, set: { save(days: $0) }))
            }
        }
    }

    private func save(minutes: UInt32? = nil, days: UInt8? = nil, rule: TimedRule? = nil) {
        let h = summary.habit
        model.updateHabit(Habit(id: h.id, title: h.title, goal: minutes ?? h.goal, days: days ?? h.days, period: h.period, timed: rule ?? h.timed))
    }

    /// Moving an end inward hands the freed time to the neighbour on that side (as on Android).
    private func edit(_ id: String, _ draft: StretchDraft, on: Bool, start: Date, end: Date?) {
        let ms = { (d: Date) in Int64(d.timeIntervalSince1970 * 1000) }
        if let old = draft.segment {
            if ms(start) > old.start, let prev = draft.prev {
                model.setHabitState(id, on: prev.on, from: date(old.start), to: start)
            }
            if let end, ms(end) < old.end, let next = draft.next {
                model.setHabitState(id, on: next.on, from: end, to: date(old.end))
            }
        }
        model.setHabitState(id, on: on, from: start, to: end)
    }
}

/// A stretch being edited (or a new one when `segment` is nil), with its neighbours.
struct StretchDraft: Identifiable {
    let id = UUID()
    let segment: TimeSegment?
    let prev: TimeSegment?
    let next: TimeSegment?
    let open: Bool
}

/// State, start and end of one stretch; the current one can stay running.
private struct StretchEditor: View {
    let draft: StretchDraft
    let rule: TimedRule
    let onSave: (Bool, Date, Date?) -> Void
    let onCancel: () -> Void
    @State private var on = false
    @State private var start = Date.now
    @State private var end = Date.now
    @State private var running = false

    var body: some View {
        Form {
            Picker("State", selection: $on) {
                Text(stateName(rule, on: true)).tag(true)
                Text(stateName(rule, on: false)).tag(false)
            }
            .pickerStyle(.segmented)
            DatePicker("From", selection: $start, in: Calendar.current.startOfDay(for: .now)...Date.now, displayedComponents: .hourAndMinute)
            Toggle("Still going", isOn: $running)
            if !running {
                DatePicker("Until", selection: $end, in: start...Date.now, displayedComponents: .hourAndMinute)
            }
            HStack {
                Spacer()
                Button("Cancel", action: onCancel)
                Button("Save") { onSave(on, start, running ? nil : end) }
                    .keyboardShortcut(.defaultAction)
            }
        }
        .padding()
        .frame(width: 300)
        .onAppear {
            on = draft.segment?.on ?? !rule.usualOn
            start = draft.segment.map { date($0.start) } ?? Date.now.addingTimeInterval(-30 * 60)
            end = draft.segment.map { date($0.end) } ?? Date.now
            running = draft.open
        }
    }
}

/// Today from midnight to midnight: on in the accent color, the unusual state orange, a tick at now.
private struct DayBarView: View {
    let segments: [TimeSegment]
    let rule: TimedRule

    var body: some View {
        GeometryReader { geo in
            let start = Calendar.current.startOfDay(for: .now).timeIntervalSince1970 * 1000
            let day = 24.0 * 3_600_000
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: 4).fill(.quaternary)
                ForEach(Array(segments.enumerated()), id: \.offset) { _, s in
                    let x0 = geo.size.width * (Double(s.start) - start) / day
                    let x1 = geo.size.width * (Double(s.end) - start) / day
                    Rectangle()
                        .fill(s.on ? Color.accentColor : (s.on == rule.usualOn ? Color.secondary.opacity(0.4) : Color.orange))
                        .frame(width: max(1.5, x1 - x0))
                        .offset(x: x0)
                }
                Rectangle().fill(.primary).frame(width: 1.5)
                    .offset(x: geo.size.width * (Date.now.timeIntervalSince1970 * 1000 - start) / day)
            }
            .clipShape(RoundedRectangle(cornerRadius: 4))
        }
        .frame(height: 14)
    }
}

private func date(_ ms: Int64) -> Date { Date(timeIntervalSince1970: Double(ms) / 1000) }

private func clock(_ ms: Int64) -> String { date(ms).formatted(date: .omitted, time: .shortened) }
