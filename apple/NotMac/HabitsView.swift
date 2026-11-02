import NotCore
import SwiftUI

struct HabitsView: View {
    @Bindable var model: AppModel
    @State private var draft = ""
    @FocusState private var addFieldFocused: Bool

    var body: some View {
        VStack(spacing: 0) {
            addField
            Divider()
            if model.habits.isEmpty {
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
        .navigationSubtitle(Date.now.formatted(date: .complete, time: .omitted))
        .onAppear { addFieldFocused = true }
        .onChange(of: model.addFieldFocusRequests) { addFieldFocused = true }
    }

    private var addField: some View {
        HStack(spacing: 10) {
            Image(systemName: "plus.circle.fill")
                .foregroundStyle(.tint)
                .imageScale(.large)
            TextField("New habit", text: $draft)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($addFieldFocused)
                .onSubmit {
                    if let id = model.addHabit(draft) {
                        model.selectedHabitID = id
                    }
                    draft = ""
                }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
    }

    private var habitList: some View {
        List(selection: $model.selectedHabitID) {
            ForEach(model.habits, id: \.habit.id) { summary in
                HabitRow(summary: summary, model: model)
            }
            .onMove { source, destination in
                if let from = source.first { model.moveHabit(from: from, to: destination) }
            }
        }
        .listStyle(.inset)
        .animation(.snappy, value: model.habits.map(\.habit.id))
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first {
                Button("Show Info") {
                    model.selectedHabitID = id
                    model.showsInspector = true
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
            model.toggleHabit(id)
            return .handled
        }
    }
}

private struct HabitRow: View {
    let summary: HabitSummary
    let model: AppModel

    private var habit: Habit { summary.habit }

    var body: some View {
        HStack(spacing: 12) {
            Button { model.toggleHabit(habit.id) } label: {
                CheckRing(count: summary.count, goal: habit.goal, due: summary.due)
            }
            .buttonStyle(.plain)
            .help(summary.count >= habit.goal ? "Done today. Click to reset." : "Check in")

            VStack(alignment: .leading, spacing: 2) {
                Text(habit.title)
                    .foregroundStyle(summary.due ? .primary : .secondary)
                HStack(spacing: 8) {
                    Text(scheduleText(habit))
                    if summary.streak > 0 {
                        Label("\(summary.streak)", systemImage: "flame.fill")
                            .foregroundStyle(.orange)
                            .help("\(summary.streak)-day streak")
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
struct CheckRing: View {
    let count: UInt32
    let goal: UInt32
    let due: Bool
    var size: CGFloat = 24

    private var progress: Double { min(1, Double(count) / Double(max(goal, 1))) }

    var body: some View {
        ZStack {
            if progress >= 1 {
                Circle().fill(.tint)
                Image(systemName: "checkmark")
                    .font(.system(size: size * 0.45, weight: .bold))
                    .foregroundStyle(.white)
            } else {
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
                    DayDot(count: count, goal: summary.habit.goal,
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

            Section("Today") {
                HStack(spacing: 14) {
                    CheckRing(count: summary.count, goal: habit.goal, due: summary.due, size: 36)
                    VStack(alignment: .leading) {
                        Text("\(summary.count) of \(habit.goal)").font(.title3.monospacedDigit())
                        Text(summary.due ? "Due today" : "Not scheduled today")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    ControlGroup {
                        Button { model.logHabit(habit.id, -1) } label: { Image(systemName: "minus") }
                            .disabled(summary.count == 0)
                        Button { model.logHabit(habit.id, 1) } label: { Image(systemName: "plus") }
                    }
                    .fixedSize()
                }
            }

            Section("Schedule") {
                Stepper(value: Binding(
                    get: { Int(habit.goal) },
                    set: { save(goal: UInt32($0)) }
                ), in: 1...99) {
                    Text(habit.goal == 1 ? "Once a day" : "\(habit.goal) times a day")
                }
                WeekdayPicker(days: Binding(get: { habit.days }, set: { save(days: $0) }))
            }

            Section("Streak") {
                LabeledContent("Current", value: days(summary.streak))
                LabeledContent("Best", value: days(summary.bestStreak))
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

    private func days(_ n: UInt32) -> String { n == 1 ? "1 day" : "\(n) days" }

    private func saveTitle() {
        let trimmed = title.trimmingCharacters(in: .whitespaces)
        guard !trimmed.isEmpty, trimmed != habit.title, model.selectedHabit?.habit.id == habit.id else { return }
        model.updateHabit(Habit(id: habit.id, title: trimmed, goal: habit.goal, days: habit.days))
    }

    private func save(goal: UInt32? = nil, days: UInt8? = nil) {
        model.updateHabit(Habit(id: habit.id, title: habit.title, goal: goal ?? habit.goal, days: days ?? habit.days))
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
                            DayDot(count: history[key] ?? 0, goal: summary.habit.goal,
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

func scheduleText(_ habit: Habit) -> String {
    let days: String = switch habit.days {
    case everyDay(): "Every day"
    case 0b001_1111: "Weekdays"
    case 0b110_0000: "Weekends"
    default: (0..<7).filter { isScheduled(habit.days, $0) }.map { weekdaySymbols[$0] }.formatted(.list(type: .and))
    }
    return habit.goal > 1 ? "\(days) · \(habit.goal)×" : days
}
