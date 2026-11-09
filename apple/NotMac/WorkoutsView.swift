import Charts
import NotCore
import SwiftUI

/// Sessions, latest first, beside the selected session's exercises.
struct WorkoutsView: View {
    @Bindable var model: AppModel
    @State private var importing = false

    var body: some View {
        HSplitView {
            WorkoutList(model: model, importing: $importing)
                .frame(minWidth: 220, idealWidth: 260, maxWidth: 360)
            Group {
                if let workout = model.workout {
                    WorkoutEditor(model: model, workout: workout)
                        .id(workout.id)
                } else {
                    ContentUnavailableView {
                        Label(model.workouts.isEmpty ? "No Workouts Yet" : "No Workout Selected", systemImage: "dumbbell")
                    } description: {
                        if model.workouts.isEmpty {
                            Text("Start one when you hit the gym, or bring in what you've logged elsewhere.")
                        }
                    } actions: {
                        Button("Start Workout") { model.newWorkout() }
                        if model.workouts.isEmpty {
                            Button("Import from Text…") { importing = true }
                        }
                    }
                }
            }
            .frame(minWidth: 420, maxWidth: .infinity, maxHeight: .infinity)
        }
        .navigationTitle("Workouts")
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                RestTimerButton(timer: model.restTimer)
                Menu {
                    Button("Empty Workout") { model.newWorkout() }
                    let recent = model.workouts.filter { !$0.exerciseNames.isEmpty }.prefix(8)
                    if !recent.isEmpty {
                        Section("Repeat") {
                            ForEach(recent, id: \.id) { w in
                                Button(workoutName(w)) { model.repeatWorkout(w.id) }
                            }
                        }
                    }
                    Divider()
                    Button("Import from Text…") { importing = true }
                } label: {
                    Label("New Workout", systemImage: "plus")
                } primaryAction: {
                    model.newWorkout()
                }
                .help("New workout (⌘N)")
            }
        }
        .sheet(isPresented: $importing) { ImportSheet(model: model) }
    }
}

// MARK: - List

private struct WorkoutList: View {
    @Bindable var model: AppModel
    @Binding var importing: Bool

    var body: some View {
        List(selection: $model.selectedWorkoutID) {
            ForEach(model.workouts, id: \.id) { w in
                VStack(alignment: .leading, spacing: 3) {
                    HStack(alignment: .firstTextBaseline) {
                        Text(w.title.isEmpty ? dayLabel(w.date, today: model.today) : w.title)
                            .font(.headline)
                        Spacer()
                        if !w.title.isEmpty {
                            Text(dayLabel(w.date, today: model.today)).foregroundStyle(.secondary)
                        }
                    }
                    Text(w.exerciseNames.isEmpty ? "No exercises" : w.exerciseNames.joined(separator: " · "))
                        .font(.callout)
                        .foregroundStyle(.tertiary)
                        .lineLimit(2)
                }
                .padding(.vertical, 4)
                .tag(w.id)
            }
        }
        .listStyle(.inset)
        .animation(.snappy, value: model.workouts.map(\.id))
        .contextMenu(forSelectionType: String.self) { ids in
            if let id = ids.first {
                Button("Repeat Today") { model.repeatWorkout(id) }
                Divider()
                Button("Delete", role: .destructive) { model.deleteWorkout(id) }
            }
        }
        .onDeleteCommand { if let id = model.selectedWorkoutID { model.deleteWorkout(id) } }
    }
}

// MARK: - Editor

/// Where typing goes in the editor.
private enum Field: Hashable {
    case title
    case sets(String)
    case newExercise
}

private struct WorkoutEditor: View {
    let model: AppModel
    let workout: Workout
    @State private var title = ""
    @State private var newExercise = ""
    @FocusState private var focus: Field?

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                header
                ForEach(Array(workout.exercises.enumerated()), id: \.element.id) { index, exercise in
                    ExerciseCard(
                        model: model,
                        exercise: exercise,
                        index: index,
                        count: workout.exercises.count,
                        focus: $focus
                    )
                }
                addExercise
            }
            .padding(.horizontal, 28)
            .padding(.vertical, 22)
            .frame(maxWidth: 760, alignment: .leading)
            .frame(maxWidth: .infinity)
        }
        .onAppear {
            title = workout.title
            if workout.exercises.isEmpty { focus = .newExercise }
        }
        .onChange(of: workout.title) { _, synced in if focus != .title { title = synced } }
        .onChange(of: focus) { old, _ in if old == .title { saveTitle() } }
    }

    private var header: some View {
        HStack(alignment: .firstTextBaseline) {
            TextField("Workout", text: $title, prompt: Text(dayLabel(workout.date, today: model.today)))
                .textFieldStyle(.plain)
                .font(.largeTitle.bold())
                .focused($focus, equals: .title)
                .onSubmit(saveTitle)
            DayButton(day: workout.date, today: model.today) { day in
                model.updateWorkout(workout.id, date: day, title: workout.title)
            }
        }
        .padding(.bottom, 6)
    }

    private var addExercise: some View {
        HStack(spacing: 10) {
            Image(systemName: "plus.circle.fill")
                .foregroundStyle(.tint)
                .imageScale(.large)
            TextField("Add exercise", text: $newExercise)
                .textFieldStyle(.plain)
                .font(.title3)
                .focused($focus, equals: .newExercise)
                .textInputSuggestions {
                    let taken = Set(workout.exercises.map { $0.name.lowercased() })
                    ForEach(suggestions.filter { !taken.contains($0.lowercased()) }.prefix(8), id: \.self) { name in
                        Text(name).textInputCompletion(name)
                    }
                }
                .onSubmit {
                    guard let id = model.addExercise(newExercise, to: workout.id) else { return }
                    newExercise = ""
                    focus = .sets(id)
                }
        }
        .padding(.top, 4)
    }

    /// Names used before, matching what's typed so far.
    private var suggestions: [String] {
        let typed = newExercise.trimmingCharacters(in: .whitespaces)
        guard !typed.isEmpty else { return model.exerciseNames }
        return model.exerciseNames.filter { $0.localizedCaseInsensitiveContains(typed) }
    }

    private func saveTitle() {
        if title != workout.title { model.updateWorkout(workout.id, date: workout.date, title: title) }
    }
}

private struct ExerciseCard: View {
    let model: AppModel
    let exercise: WorkoutExercise
    let index: Int
    let count: Int
    var focus: FocusState<Field?>.Binding
    @State private var name = ""
    @State private var draft = ""
    @State private var editingLine: Int?
    @State private var lineDraft = ""
    @State private var showsHistory = false
    @FocusState private var nameFocused: Bool
    @FocusState private var lineFocused: Bool

    private var lines: [String] { exercise.lines.map(\.raw) }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                TextField("Exercise", text: $name)
                    .textFieldStyle(.plain)
                    .font(.headline)
                    .focused($nameFocused)
                    .onSubmit(saveName)
                    .onChange(of: nameFocused) { _, focused in if !focused { saveName() } }
                Spacer(minLength: 0)
                Button { showsHistory = true } label: {
                    Image(systemName: "chart.line.uptrend.xyaxis")
                }
                .buttonStyle(.borderless)
                .help("History")
                .popover(isPresented: $showsHistory, arrowEdge: .bottom) {
                    ExerciseHistory(name: exercise.name, logs: model.exerciseHistory(exercise.name), today: model.today)
                }
                Menu {
                    Button("Move Up") { model.moveExercise(exercise.id, to: index - 1) }
                        .disabled(index == 0)
                    Button("Move Down") { model.moveExercise(exercise.id, to: index + 1) }
                        .disabled(index == count - 1)
                    Divider()
                    Button("Delete Exercise", role: .destructive) { model.deleteExercise(exercise.id) }
                } label: {
                    Image(systemName: "ellipsis.circle")
                }
                .menuStyle(.borderlessButton)
                .menuIndicator(.hidden)
                .fixedSize()
            }

            if let previous = exercise.previous {
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text("Last · \(dayLabel(previous.date, today: model.today))")
                        .foregroundStyle(.secondary)
                    Text(previous.lines.map(\.label).joined(separator: "  ·  "))
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Spacer(minLength: 0)
                    if exercise.sets.isEmpty {
                        Button("Same Again") { model.updateExercise(exercise.id, name: exercise.name, sets: previous.sets) }
                            .buttonStyle(.borderless)
                            .foregroundStyle(.tint)
                            .help("Copy last time's sets")
                    }
                }
                .font(.callout)
            }

            ForEach(Array(exercise.lines.enumerated()), id: \.offset) { i, line in
                if editingLine == i {
                    TextField("Sets", text: $lineDraft)
                        .textFieldStyle(.roundedBorder)
                        .focused($lineFocused)
                        .onAppear { lineFocused = true }
                        .onSubmit { commitLine(i) }
                        .onExitCommand { editingLine = nil }
                        .onChange(of: lineFocused) { _, focused in if !focused { commitLine(i) } }
                } else {
                    SetLineRow(line: line)
                        .contentShape(.rect)
                        .onTapGesture(count: 2) {
                            lineDraft = line.raw
                            editingLine = i
                        }
                        .contextMenu {
                            Button("Edit") {
                                lineDraft = line.raw
                                editingLine = i
                            }
                            Button("Delete", role: .destructive) { replaceLine(i, with: nil) }
                        }
                }
            }

            addSets
        }
        .padding(14)
        .background(.background.secondary, in: .rect(cornerRadius: 12))
        .onAppear { name = exercise.name }
        .onChange(of: exercise.name) { _, synced in if !nameFocused { name = synced } }
    }

    private var addSets: some View {
        let preview = draft.trimmingCharacters(in: .whitespaces).isEmpty ? nil : parseSets(text: draft).first
        return HStack(spacing: 8) {
            Image(systemName: "plus")
                .foregroundStyle(.secondary)
                .imageScale(.small)
            TextField("Sets", text: $draft, prompt: Text(exercise.previous?.lines.last { $0.parsed }?.raw ?? "24kg x 10 x 3"))
                .textFieldStyle(.plain)
                .focused(focus, equals: .sets(exercise.id))
                .onSubmit(addLine)
            if let preview {
                Text(preview.parsed ? preview.label : "Note")
                    .font(.callout)
                    .foregroundStyle(preview.parsed ? AnyShapeStyle(.tint) : AnyShapeStyle(.tertiary))
                    .help(preview.parsed ? "Read as sets" : "Not read as sets; kept as a note")
            }
        }
        .padding(.top, exercise.lines.isEmpty ? 0 : 2)
    }

    private func addLine() {
        let text = draft.trimmingCharacters(in: .whitespaces)
        guard !text.isEmpty else { return }
        let parsed = parseSets(text: text).first
        model.updateExercise(exercise.id, name: exercise.name, sets: (lines + [text]).joined(separator: "\n"))
        draft = ""
        if parsed?.parsed == true, parsed?.warmup == false, model.restTimer.startsAfterSet {
            model.restTimer.start()
        }
    }

    private func commitLine(_ i: Int) {
        guard editingLine == i else { return }
        editingLine = nil
        let text = lineDraft.trimmingCharacters(in: .whitespaces)
        if text != lines[i] { replaceLine(i, with: text.isEmpty ? nil : text) }
    }

    private func replaceLine(_ i: Int, with text: String?) {
        var all = lines
        guard all.indices.contains(i) else { return }
        if let text { all[i] = text } else { all.remove(at: i) }
        model.updateExercise(exercise.id, name: exercise.name, sets: all.joined(separator: "\n"))
    }

    private func saveName() {
        let trimmed = name.trimmingCharacters(in: .whitespaces)
        if trimmed.isEmpty { name = exercise.name; return }
        if trimmed != exercise.name { model.updateExercise(exercise.id, name: trimmed, sets: exercise.sets) }
    }
}

private struct SetLineRow: View {
    let line: SetLine

    var body: some View {
        HStack(spacing: 8) {
            Circle()
                .fill(line.parsed ? (line.warmup ? AnyShapeStyle(.tertiary) : AnyShapeStyle(.tint)) : AnyShapeStyle(.clear))
                .frame(width: 6, height: 6)
            Text(line.label)
                .foregroundStyle(line.parsed && !line.warmup ? .primary : .secondary)
                .italic(!line.parsed)
            Spacer(minLength: 0)
        }
        .font(.body.monospacedDigit())
    }
}

/// "Today" / "Mon 2 Nov" as a quiet button; opens a calendar to move the session to another day.
private struct DayButton: View {
    let day: String
    let today: String
    let onPick: (String) -> Void
    @State private var picking = false

    var body: some View {
        Button { picking = true } label: {
            // The actual date: the title above already says "Today" or "Yesterday".
            Label(parseDay(day)?.formatted(.dateTime.weekday(.wide).day().month(.wide)) ?? day, systemImage: "calendar")
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(.quaternary.opacity(0.6), in: .capsule)
        }
        .buttonStyle(.plain)
        .foregroundStyle(.secondary)
        .help("Change the date")
        .popover(isPresented: $picking, arrowEdge: .bottom) {
            VStack(spacing: 8) {
                DatePicker("Date", selection: Binding(
                    get: { parseDay(day) ?? .now },
                    set: {
                        onPick(AppModel.dayString($0))
                        picking = false
                    }
                ), in: ...Date.now, displayedComponents: .date)
                .datePickerStyle(.graphical)
                .labelsHidden()
                if day != today {
                    Button("Today") {
                        onPick(today)
                        picking = false
                    }
                    .buttonStyle(.borderless)
                    .foregroundStyle(.tint)
                }
            }
            .padding(12)
        }
    }
}

// MARK: - History

private struct ExerciseHistory: View {
    let name: String
    let logs: [ExerciseLog]
    let today: String

    private var points: [(date: Date, weight: Double)] {
        logs.reversed().compactMap { log in
            guard let w = log.topWeight, let d = parseDay(log.date) else { return nil }
            return (d, w)
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(name).font(.headline)
            if logs.isEmpty {
                Text("Nothing logged yet.").foregroundStyle(.secondary)
            } else {
                if points.count > 1 {
                    Chart(points, id: \.date) { p in
                        LineMark(x: .value("Date", p.date), y: .value("Top set", p.weight))
                            .interpolationMethod(.monotone)
                        PointMark(x: .value("Date", p.date), y: .value("Top set", p.weight))
                    }
                    .chartYScale(domain: .automatic(includesZero: false))
                    .frame(height: 120)
                }
                if let best = logs.compactMap(\.topWeight).max() {
                    Text("Best \(formatWeight(best)) kg · \(logs.count) sessions")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
                ScrollView {
                    VStack(alignment: .leading, spacing: 10) {
                        ForEach(logs, id: \.workoutId) { log in
                            VStack(alignment: .leading, spacing: 2) {
                                Text(dayLabel(log.date, today: today)).font(.callout.weight(.semibold))
                                ForEach(Array(log.lines.enumerated()), id: \.offset) { _, line in
                                    Text(line.label)
                                        .font(.callout.monospacedDigit())
                                        .foregroundStyle(line.warmup || !line.parsed ? .secondary : .primary)
                                }
                            }
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxHeight: 260)
            }
        }
        .padding(16)
        .frame(width: 320)
    }
}

// MARK: - Rest timer

private struct RestTimerButton: View {
    let timer: RestTimer

    var body: some View {
        Menu {
            Section("Rest For") {
                ForEach(RestTimer.presets, id: \.self) { s in
                    Button(RestTimer.format(s)) { timer.start(seconds: s) }
                }
            }
            if timer.isRunning {
                Button("Add 15 Seconds") { timer.extend(by: 15) }
                Button("Take Off 15 Seconds") { timer.extend(by: -15) }
                Button("Stop") { timer.stop() }
            }
            Divider()
            Toggle("Start After Each Set", isOn: Binding(get: { timer.startsAfterSet }, set: { timer.startsAfterSet = $0 }))
        } label: {
            if let ends = timer.endsAt {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Label(RestTimer.format(max(0, Int(ends.timeIntervalSince(context.date).rounded(.up)))), systemImage: "timer")
                        .labelStyle(.titleAndIcon)
                        .monospacedDigit()
                }
            } else {
                Label("Rest \(RestTimer.format(timer.duration))", systemImage: "timer")
            }
        } primaryAction: {
            timer.isRunning ? timer.stop() : timer.start()
        }
        .help(timer.isRunning ? "Stop the rest timer" : "Start a \(RestTimer.format(timer.duration)) rest")
    }
}

// MARK: - Import

private struct ImportSheet: View {
    let model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var text = ""
    @State private var result: Int?

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Import Workouts").font(.title2.bold())
            Text("Paste a log: a date like 30/10 starts a workout, a line like “24kg x 10 x 3” adds sets, any other line names an exercise.")
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
            TextEditor(text: $text)
                .font(.body.monospaced())
                .frame(minHeight: 260)
                .overlay(RoundedRectangle(cornerRadius: 6).stroke(.quaternary))
            HStack {
                if let result {
                    Text(result == 0 ? "No dated workouts found." : "Added \(result) workout\(result == 1 ? "" : "s").")
                        .foregroundStyle(result == 0 ? .orange : .secondary)
                }
                Spacer()
                Button("Cancel", role: .cancel) { dismiss() }
                    .keyboardShortcut(.cancelAction)
                Button("Import") {
                    let added = model.importWorkouts(text)
                    result = added
                    if added > 0 { dismiss() }
                }
                .keyboardShortcut(.defaultAction)
                .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
            }
        }
        .padding(20)
        .frame(width: 520)
    }
}

// MARK: - Formatting

func workoutName(_ w: WorkoutSummary) -> String {
    let day = parseDay(w.date)?.formatted(.dateTime.day().month(.abbreviated)) ?? w.date
    let what = w.title.isEmpty ? w.exerciseNames.prefix(2).joined(separator: ", ") : w.title
    return "\(what) · \(day)"
}

/// "Today", "Yesterday", "Mon 2 Nov", or with the year when it isn't this one.
func dayLabel(_ day: String, today: String) -> String {
    guard let date = parseDay(day), let now = parseDay(today) else { return day }
    let calendar = Calendar.current
    let days = calendar.dateComponents([.day], from: date, to: now).day ?? 0
    switch days {
    case 0: return "Today"
    case 1: return "Yesterday"
    default:
        let sameYear = calendar.component(.year, from: date) == calendar.component(.year, from: now)
        return sameYear
            ? date.formatted(.dateTime.weekday(.abbreviated).day().month(.abbreviated))
            : date.formatted(.dateTime.day().month(.abbreviated).year())
    }
}

/// `YYYY-MM-DD` at local midnight.
func parseDay(_ s: String) -> Date? {
    let parts = s.split(separator: "-").compactMap { Int($0) }
    guard parts.count == 3 else { return nil }
    return Calendar.current.date(from: DateComponents(year: parts[0], month: parts[1], day: parts[2]))
}

func formatWeight(_ w: Double) -> String {
    w.formatted(.number.precision(.fractionLength(0...2)))
}
