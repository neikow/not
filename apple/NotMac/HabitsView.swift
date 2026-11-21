import NotCore
import SwiftUI

struct HabitsView: View {
    @Bindable var model: AppModel
    @State private var draft = ""
    /// What the add field makes: check-ins, or a timed habit.
    @State private var timed = false
    @FocusState private var addFieldFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            addField
            DayBar(model: model)
            Divider()
            if model.shownHabits.isEmpty {
                ContentUnavailableView(
                    "No habits yet",
                    systemImage: "repeat",
                    description: Text("Type above and press Return. Check in each day to build a streak.")
                )
            } else {
                habitList
            }
        }
        .navigationTitle("Habits")
        .navigationSubtitle(subtitle)
        .onAppear { addFieldFocused = true }
        .onChange(of: model.addFieldFocusRequests) { addFieldFocused = true }
    }

    /// "Saturday 21 November · 2 of 4 done" for the day shown; timed habits judged as of now.
    private var subtitle: String {
        let date = (AppModel.dayFormatter.date(from: model.habitDay) ?? .now).formatted(.dateTime.weekday(.wide).day().month(.wide))
        let due = model.shownHabits.filter(\.inDayCount)
        let done = due.count { $0.doneAt(model.clock, loadedAt: model.habitsLoadedAt, isToday: model.isHabitToday) }
        let progress = if due.isEmpty {
            model.isHabitToday ? "Nothing due today" : "Nothing was due"
        } else if done == due.count {
            model.isHabitToday ? "All done for today" : "All done"
        } else {
            "\(done) of \(due.count) done"
        }
        return "\(date) · \(progress)"
    }

    private var addField: some View {
        HStack(spacing: 10) {
            Image(systemName: "plus.circle.fill")
                .foregroundStyle(.tint)
                .imageScale(.large)
            TextField(timed ? "New timed habit, like “Wear retainer”" : "New habit", text: $draft)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($addFieldFocused)
                .onSubmit {
                    if let id = timed ? model.addTimedHabit(draft) : model.addHabit(draft) {
                        model.selectedHabitID = id
                        // A timed habit's goal and rule are set next, in the inspector.
                        if timed { model.showsInspector = true }
                    }
                    draft = ""
                }
            Picker("Kind", selection: $timed) {
                Label("Check-ins", systemImage: "checkmark.circle").tag(false)
                Label("Timed", systemImage: "timer").tag(true)
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .fixedSize()
            .help("Check-ins count taps; timed habits switch on and off with a daily goal in time")
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
    }

    private var habitList: some View {
        List(selection: $model.selectedHabitID) {
            ForEach(model.shownHabits, id: \.habit.id) { summary in
                HabitRow(summary: summary, model: model)
            }
            .onMove { source, destination in
                if let from = source.first { model.moveHabit(from: from, to: destination) }
            }
        }
        .listStyle(.inset)
        .animation(.snappy, value: model.shownHabits.map(\.habit.id))
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first {
                Button("Show Info") {
                    model.selectedHabitID = id
                    model.showsInspector = true
                }
                if model.isHabitToday, let summary = model.shownHabits.first(where: { $0.habit.id == id }), let rule = summary.habit.timed {
                    let on = summary.timed?.on ?? false
                    Button(switchLabel(rule, on: on)) { model.switchHabit(id, on: !on) }
                    Menu("\(stateName(rule, on: !on)) Since") {
                        ForEach([5, 15, 30, 60], id: \.self) { minutes in
                            Button("\(minutes) Minutes Ago") {
                                model.setHabitState(id, on: !on, from: Date.now.addingTimeInterval(Double(-minutes * 60)))
                            }
                        }
                    }
                }
                Divider()
                Button("Delete", role: .destructive) { model.deleteHabit(id) }
            }
        } primaryAction: { ids in
            model.selectedHabitID = ids.first
            model.showsInspector = true
        }
        .onDeleteCommand { if let id = model.selectedHabitID { model.deleteHabit(id) } }
        .onKeyPress(.space) {
            guard let id = model.selectedHabitID else { return .ignored }
            model.toggleHabit(id, day: model.habitDay)
            return .handled
        }
    }
}

/// The last 7 days, oldest first, the shown one filled in: pick one to catch up on check-ins.
/// ⌘← and ⌘→ step through them.
private struct DayBar: View {
    let model: AppModel

    var body: some View {
        let days = model.habitDays
        HStack(spacing: 4) {
            ForEach(days, id: \.self) { day in
                let date = AppModel.dayFormatter.date(from: day) ?? .now
                let selected = day == model.habitDay
                let isToday = day == model.today
                Button { model.habitDay = day } label: {
                    VStack(spacing: 1) {
                        Text(isToday ? "Today" : date.formatted(.dateTime.weekday(.abbreviated)))
                            .font(.caption2)
                            .foregroundStyle(selected ? AnyShapeStyle(.white) : isToday ? AnyShapeStyle(.tint) : AnyShapeStyle(.secondary))
                        Text(date.formatted(.dateTime.day()))
                            .font(.callout.weight(.semibold).monospacedDigit())
                            .foregroundStyle(selected ? AnyShapeStyle(.white) : AnyShapeStyle(.primary))
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 5)
                    .background(selected ? AnyShapeStyle(.tint) : AnyShapeStyle(.clear), in: .rect(cornerRadius: 9))
                    .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .help(isToday ? "Today" : "Check in on \(date.formatted(date: .complete, time: .omitted))")
            }
        }
        .frame(maxWidth: 520)
        .padding(.horizontal, 14)
        .padding(.bottom, 8)
        .animation(.snappy, value: model.habitDay)
        .background {
            // Keyboard: step a day back or forward.
            Group {
                Button("") { step(-1, days) }.keyboardShortcut(.leftArrow, modifiers: .command)
                Button("") { step(1, days) }.keyboardShortcut(.rightArrow, modifiers: .command)
            }
            .opacity(0)
            .allowsHitTesting(false)
        }
    }

    private func step(_ by: Int, _ days: [String]) {
        guard let at = days.firstIndex(of: model.habitDay), days.indices.contains(at + by) else { return }
        model.habitDay = days[at + by]
    }
}

private struct HabitRow: View {
    let summary: HabitSummary
    let model: AppModel

    private var habit: Habit { summary.habit }

    var body: some View {
        if habit.timed != nil {
            TimedHabitRow(summary: summary, model: model)
        } else {
            countRow
        }
    }

    private var countRow: some View {
        HStack(spacing: 12) {
            Button { model.toggleHabit(habit.id, day: model.habitDay) } label: {
                HabitRing(summary: summary)
            }
            .buttonStyle(.plain)
            .help(summary.doneToday ? (habit.isDaily ? "Done today. Click to reset." : "Checked in today. Click to take it back.") : "Check in")

            VStack(alignment: .leading, spacing: 2) {
                Text(habit.title)
                    // Weekly and monthly habits can be done any day, so they never read as off today.
                    .foregroundStyle(summary.due || !habit.isDaily ? .primary : .secondary)
                HStack(spacing: 8) {
                    Text(scheduleText(habit, summary))
                    if summary.streak > 0 {
                        Label("\(summary.streak)\(streakUnit(habit.period))", systemImage: "flame.fill")
                            .foregroundStyle(.orange)
                            .help("\(streakText(summary.streak, habit.period)) streak")
                    }
                }
                .font(.caption)
                .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            WeekStrip(summary: summary)
        }
        .padding(.vertical, 4)
    }
}

/// Today's progress: a ring that fills with each check-in and turns solid when the goal is met.
/// The ring for a habit's goal: the day's for daily habits, the week's or month's otherwise.
struct HabitRing: View {
    let summary: HabitSummary
    var size: CGFloat = 24

    var body: some View {
        let habit = summary.habit
        if habit.timed != nil {
            TimelineView(.periodic(from: .now, by: 30)) { context in
                TimedRingView(live: summary.timedLive(now: context.date, loadedAt: .now), size: size)
            }
        } else if habit.isDaily {
            CheckRing(count: summary.count, goal: habit.goal, due: summary.due, size: size)
        } else {
            CheckRing(count: summary.progress, goal: habit.goal, due: true, size: size, checkedToday: summary.count > 0)
        }
    }
}

/// [checkedToday] tints the inside, for goals over a week or month that the day already counted toward.
struct CheckRing: View {
    let count: UInt32
    let goal: UInt32
    let due: Bool
    var size: CGFloat = 24
    var checkedToday = false

    private var progress: Double { min(1, Double(count) / Double(max(goal, 1))) }

    var body: some View {
        ZStack {
            if progress >= 1 {
                Circle().fill(.tint)
                Image(systemName: "checkmark")
                    .font(.system(size: size * 0.45, weight: .bold))
                    .foregroundStyle(.white)
            } else {
                if checkedToday { Circle().fill(.tint.opacity(0.18)) }
                Circle()
                    .strokeBorder(.secondary.opacity(due ? 0.5 : 0.25),
                                  style: StrokeStyle(lineWidth: 1.5, dash: due ? [] : [2, 2]))
                Circle()
                    .trim(from: 0, to: progress)
                    .stroke(.tint, style: StrokeStyle(lineWidth: 3, lineCap: .round))
                    .rotationEffect(.degrees(-90))
                    .padding(1.5)
                if count > 0 {
                    Text("\(count)")
                        .font(.system(size: size * 0.42, weight: .semibold).monospacedDigit())
                        .foregroundStyle(.tint)
                }
            }
        }
        .frame(width: size, height: size)
        .contentShape(.circle)
        .animation(.snappy, value: count)
    }
}

/// The last 7 days, oldest first; today is outlined.
private struct WeekStrip: View {
    let summary: HabitSummary

    var body: some View {
        HStack(spacing: 5) {
            ForEach(Array(summary.week.enumerated()), id: \.offset) { index, count in
                let back = 6 - index
                let weekday = (todayWeekday() - back + 7) % 7
                VStack(spacing: 3) {
                    DayDot(count: count, goal: summary.habit.dayGoal,
                           scheduled: isScheduled(summary.habit.days, weekday))
                        .overlay {
                            if back == 0 { Circle().strokeBorder(.primary.opacity(0.35), lineWidth: 1).padding(-3) }
                        }
                    Text(weekdaySymbols[weekday].prefix(1))
                        .font(.system(size: 9))
                        .foregroundStyle(.tertiary)
                }
            }
        }
    }
}

struct DayDot: View {
    let count: UInt32
    let goal: UInt32
    let scheduled: Bool
    var size: CGFloat = 9

    var body: some View {
        Circle()
            .fill(fill)
            .frame(width: size, height: size)
    }

    private var fill: AnyShapeStyle {
        if count >= goal { return AnyShapeStyle(.tint) }
        if count > 0 { return AnyShapeStyle(.tint.opacity(0.4)) }
        return AnyShapeStyle(.secondary.opacity(scheduled ? 0.25 : 0.08))
    }
}

// MARK: - Inspector

struct HabitInspector: View {
    let model: AppModel

    var body: some View {
        if let summary = model.selectedHabit {
            HabitDetail(model: model, summary: summary)
                .id(summary.habit.id)
        } else {
            ContentUnavailableView("No Habit Selected", systemImage: "sidebar.trailing")
        }
    }
}

private struct HabitDetail: View {
    let model: AppModel
    let summary: HabitSummary
    @State private var title = ""

    private var habit: Habit { summary.habit }

    var body: some View {
        Form {
            Section {
                TextField("Title", text: $title, axis: .vertical)
                    .font(.title3.weight(.semibold))
                    .onSubmit(saveTitle)
            }

            if habit.timed != nil {
                // Timed habits are about now: today's summary, whatever day the list shows.
                TimedDetailSections(model: model, summary: model.habits.first { $0.habit.id == habit.id } ?? summary)
            } else {
            Section(model.isHabitToday ? "Today" : (AppModel.dayFormatter.date(from: model.habitDay) ?? .now).formatted(.dateTime.weekday(.wide).day().month())) {
                HStack(spacing: 14) {
                    HabitRing(summary: summary, size: 36)
                    VStack(alignment: .leading) {
                        if habit.isDaily {
                            Text("\(summary.count) of \(habit.goal)").font(.title3.monospacedDigit())
                            Text(summary.due ? (model.isHabitToday ? "Due today" : "Was due") : (model.isHabitToday ? "Not scheduled today" : "Wasn't scheduled"))
                                .font(.caption).foregroundStyle(.secondary)
                        } else {
                            Text("\(summary.progress) of \(habit.goal) this \(periodName(habit.period))").font(.title3.monospacedDigit())
                            Text(periodStatus(summary)).font(.caption).foregroundStyle(.secondary)
                        }
                    }
                    Spacer()
                    ControlGroup {
                        Button { model.logHabit(habit.id, -1, day: model.habitDay) } label: { Image(systemName: "minus") }
                            .disabled(summary.count == 0)
                        Button { model.logHabit(habit.id, 1, day: model.habitDay) } label: { Image(systemName: "plus") }
                    }
                    .fixedSize()
                }
            }

            Section("Schedule") {
                Stepper(value: Binding(
                    get: { Int(habit.goal) },
                    set: { save(goal: UInt32($0)) }
                ), in: 1...99) {
                    Text(goalText(habit.goal, habit.period))
                }
                Picker("Counts", selection: Binding(get: { habit.period }, set: { save(period: $0) })) {
                    Text("Per day").tag(HabitPeriod.day)
                    Text("Per week").tag(HabitPeriod.week)
                    Text("Per month").tag(HabitPeriod.month)
                }
                .pickerStyle(.segmented)
                // Only daily habits keep to weekdays; a week's or month's goal takes any day.
                if habit.isDaily {
                    WeekdayPicker(days: Binding(get: { habit.days }, set: { save(days: $0) }))
                }
            }
            }

            Section("Streak") {
                LabeledContent("Current", value: streakText(summary.streak, habit.period))
                LabeledContent("Best", value: streakText(summary.bestStreak, habit.period))
            }

            Section("Last 18 weeks") {
                HistoryGrid(model: model, summary: summary, weeks: 18)
                    .frame(maxWidth: .infinity, alignment: .center)
            }

            Section {
                Button("Delete Habit", role: .destructive) { model.deleteHabit(habit.id) }
            }
        }
        .formStyle(.grouped)
        .onAppear { title = habit.title }
        .onChange(of: habit.title) { _, synced in title = synced }
        .onDisappear(perform: saveTitle)
    }

    private func saveTitle() {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, trimmed != habit.title, model.selectedHabit?.habit.id == habit.id else { return }
        model.updateHabit(Habit(id: habit.id, title: trimmed, goal: habit.goal, days: habit.days, period: habit.period, timed: habit.timed))
    }

    private func save(goal: UInt32? = nil, days: UInt8? = nil, period: HabitPeriod? = nil) {
        model.updateHabit(Habit(id: habit.id, title: habit.title, goal: goal ?? habit.goal, days: days ?? habit.days,
                                period: period ?? habit.period, timed: habit.timed))
    }
}

/// Seven toggles, Monday first. At least one day stays selected.
struct WeekdayPicker: View {
    @Binding var days: UInt8

    var body: some View {
        HStack(spacing: 4) {
            ForEach(0..<7, id: \.self) { weekday in
                let on = isScheduled(days, weekday)
                Button {
                    let next = days ^ (1 << weekday)
                    if next != 0 { days = next }
                } label: {
                    Text(weekdaySymbols[weekday].prefix(2))
                        .font(.caption.weight(.medium))
                        .frame(maxWidth: .infinity, minHeight: 24)
                        .background(on ? AnyShapeStyle(.tint) : AnyShapeStyle(.quaternary), in: .capsule)
                        .foregroundStyle(on ? .white : .secondary)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

/// Weeks as columns (oldest left), Monday on top; today is the last filled cell.
private struct HistoryGrid: View {
    let model: AppModel
    let summary: HabitSummary
    let weeks: Int

    var body: some View {
        let calendar = Calendar.current
        let today = calendar.startOfDay(for: .now)
        let start = calendar.date(byAdding: .day, value: -(todayWeekday() + 7 * (weeks - 1)), to: today)!
        // Re-read when today's count changes; past days only change through sync, which also
        // changes the summary.
        let history = model.habitHistory(summary.habit.id, from: start)
        let _ = summary.count

        Grid(horizontalSpacing: 3, verticalSpacing: 3) {
            ForEach(0..<7, id: \.self) { weekday in
                GridRow {
                    ForEach(0..<weeks, id: \.self) { week in
                        let date = calendar.date(byAdding: .day, value: week * 7 + weekday, to: start)!
                        if date <= today {
                            let key = AppModel.dayString(date)
                            DayDot(count: history[key] ?? 0, goal: summary.habit.dayGoal,
                                   scheduled: isScheduled(summary.habit.days, weekday), size: 11)
                                .help("\(date.formatted(date: .abbreviated, time: .omitted)): \(history[key] ?? 0)")
                        } else {
                            Color.clear.frame(width: 11, height: 11)
                        }
                    }
                }
            }
        }
    }
}

// MARK: - Helpers

/// Monday first.
let weekdaySymbols: [String] = {
    let symbols = Calendar.current.shortWeekdaySymbols // Sunday first
    return Array(symbols[1...] + symbols[..<1])
}()

/// 0 = Monday … 6 = Sunday.
func todayWeekday() -> Int {
    (Calendar.current.component(.weekday, from: .now) + 5) % 7
}

func isScheduled(_ days: UInt8, _ weekday: Int) -> Bool {
    days & (1 << weekday) != 0
}

func scheduleText(_ habit: Habit, _ summary: HabitSummary) -> String {
    if !habit.isDaily { return "\(goalText(habit.goal, habit.period)) · \(summary.progress) so far" }
    let days: String = switch habit.days {
    case everyDay(): "Every day"
    case 0b001_1111: "Weekdays"
    case 0b110_0000: "Weekends"
    default: (0..<7).filter { isScheduled(habit.days, $0) }.map { weekdaySymbols[$0] }.formatted(.list(type: .and))
    }
    return habit.goal > 1 ? "\(days) · \(habit.goal)×" : days
}

// MARK: - Goal periods

// What a habit's goal period means for the day: daily habits count per scheduled day, weekly and
// monthly ones on any day, toward a total. Mirrors HabitRules.kt on Android.

extension Habit {
    var isDaily: Bool { period == .day }

    /// Check-ins that fill one day's dot: the goal for daily habits, one for the others.
    var dayGoal: UInt32 { isDaily ? goal : 1 }
}

extension HabitSummary {
    /// On the day's to-do: a daily habit on a scheduled day, or a weekly or monthly one that needs
    /// the day to still make its goal, or got a check-in that day.
    var onToday: Bool { habit.isDaily ? due : due || count > 0 }

    /// Done as far as the day goes: the day's goal met, or for weekly and monthly ones, a check-in.
    var doneToday: Bool { habit.timed != nil ? done : habit.isDaily ? count >= habit.goal : count > 0 || done }

    /// Counted in the day's "2 of 5 done": every habit in the list, except a daily or timed one on a
    /// day it isn't scheduled with nothing logged (or tracked), so rest days can be all done.
    var inDayCount: Bool {
        if !habit.isDaily { return true }
        if habit.timed != nil { return due || (timed.map { $0.onMs + $0.offMs > 0 } ?? false) }
        return due || count > 0
    }

    /// On the day's to-do and not done yet: what badges count.
    var pending: Bool { onToday && !doneToday }

    /// `doneToday` as of `now`, for a summary loaded at `loadedAt`: a timed habit's running stretch
    /// keeps counting between reloads, so it can meet its goal (or use up its allowance) meanwhile.
    /// A past day nothing was tracked on isn't done, even within an allowance.
    func doneAt(_ now: Date, loadedAt: Date, isToday: Bool = true) -> Bool {
        guard let live = timedLive(now: now, loadedAt: loadedAt, isToday: isToday) else { return doneToday }
        return live.met && (live.tracked || isToday)
    }

    /// `pending` as of `now`; see `doneAt`.
    func pendingAt(_ now: Date, loadedAt: Date) -> Bool { onToday && !doneAt(now, loadedAt: loadedAt) }
}

func periodName(_ period: HabitPeriod) -> String {
    switch period {
    case .day: "day"
    case .week: "week"
    case .month: "month"
    }
}

/// "Once a day", "3 times a week", "Twice a month".
func goalText(_ goal: UInt32, _ period: HabitPeriod) -> String {
    let times = switch goal {
    case 1: "Once"
    case 2: "Twice"
    default: "\(goal) times"
    }
    return "\(times) a \(periodName(period))"
}

/// "4 days", "1 week", "2 months".
func streakText(_ n: UInt32, _ period: HabitPeriod) -> String {
    "\(n) \(periodName(period))\(n == 1 ? "" : "s")"
}

/// After the flame: nothing for days, "w" or "mo" so weeks and months don't read as days.
func streakUnit(_ period: HabitPeriod) -> String {
    switch period {
    case .day: ""
    case .week: "w"
    case .month: "mo"
    }
}

/// Under a weekly or monthly habit's progress: how the day stands.
func periodStatus(_ summary: HabitSummary) -> String {
    if summary.count > 1 { return "\(summary.count) check-ins today" }
    if summary.count == 1 { return "Checked in today" }
    if summary.done { return "Goal met" }
    if summary.due { return "Needed today to make it" }
    return "Any day counts"
}
