import AppKit
import NotCore
import SwiftUI

/// "Tomorrow 9:00 · Daily".
func scheduleLabel(due: String?, time: UInt32?, repeat rule: String?, today: String) -> String? {
    guard let due else { return nil }
    let date = (try? dueLabel(due: due, dueTime: time, today: today)) ?? due
    return ([date] + [rule.flatMap(repeatLabel)].compactMap { $0 }).joined(separator: " · ")
}

func clockLabel(_ minutes: UInt32) -> String { String(format: "%d:%02d", minutes / 60, minutes % 60) }

func priorityName(_ priority: UInt8) -> String {
    switch priority {
    case 3: "High"
    case 2: "Medium"
    case 1: "Low"
    default: "None"
    }
}

func priorityColor(_ priority: UInt8) -> Color? {
    switch priority {
    case 3: .red
    case 2: .orange
    case 1: .blue
    default: nil
    }
}

/// Quick-add text as typed, plus the recognised words the user chose to keep as plain text.
struct TaskDraft {
    var text = ""
    var dismissed: [String] = []

    mutating func clear() {
        text = ""
        dismissed = []
    }
}

/// A one-line field that tints the words the parser recognised ("tomorrow 9am", "#work", "!1").
/// SwiftUI's `TextField` can't style ranges, so this wraps `NSTextField` and colours its field
/// editor's text while editing; the string itself is never changed.
struct TaskInputField: NSViewRepresentable {
    @Binding var text: String
    var placeholder: String
    var tokens: [ParsedToken]
    var font: NSFont = .preferredFont(forTextStyle: .title3)
    /// Bump to move keyboard focus here.
    var focusRequests = 0
    var onSubmit: () -> Void

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField()
        field.isBordered = false
        field.drawsBackground = false
        field.focusRingType = .none
        field.font = font
        field.placeholderString = placeholder
        field.delegate = context.coordinator
        field.cell?.isScrollable = true
        field.cell?.wraps = false
        field.lineBreakMode = .byClipping
        field.setContentHuggingPriority(.defaultLow, for: .horizontal)
        return field
    }

    func updateNSView(_ field: NSTextField, context: Context) {
        context.coordinator.parent = self
        field.placeholderString = placeholder
        if field.stringValue != text { field.stringValue = text }
        highlight(field)
        if context.coordinator.focusRequests != focusRequests {
            context.coordinator.focusRequests = focusRequests
            DispatchQueue.main.async { field.window?.makeFirstResponder(field) }
        }
    }

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    fileprivate func highlight(_ field: NSTextField) {
        guard let editor = field.currentEditor() as? NSTextView, let storage = editor.textStorage else { return }
        let all = NSRange(location: 0, length: storage.length)
        storage.beginEditing()
        storage.removeAttribute(.backgroundColor, range: all)
        storage.addAttribute(.foregroundColor, value: NSColor.labelColor, range: all)
        for token in tokens {
            // Offsets are UTF-16, like NSString's.
            let range = NSRange(location: Int(token.start), length: Int(token.end - token.start))
            guard NSMaxRange(range) <= storage.length else { continue }
            storage.addAttributes([
                .foregroundColor: NSColor.controlAccentColor,
                .backgroundColor: NSColor.controlAccentColor.withAlphaComponent(0.16),
            ], range: range)
        }
        storage.endEditing()
    }

    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: TaskInputField
        var focusRequests: Int

        init(_ parent: TaskInputField) {
            self.parent = parent
            focusRequests = parent.focusRequests
        }

        func controlTextDidChange(_ note: Notification) {
            guard let field = note.object as? NSTextField else { return }
            parent.text = field.stringValue
        }

        func control(_ control: NSControl, textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            parent.onSubmit()
            return true
        }
    }
}

/// One chip per recognised word; ✕ keeps that word in the title instead.
struct TokenChips: View {
    let parsed: ParsedTask?
    let model: AppModel
    var onDismiss: (ParsedToken) -> Void

    var body: some View {
        if let parsed, !parsed.tokens.isEmpty {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach(parsed.tokens, id: \.start) { token in
                        let (icon, label) = describe(token, parsed)
                        let tint = token.kind == .priority ? priorityColor(parsed.priority) ?? .accentColor : .accentColor
                        HStack(spacing: 4) {
                            Image(systemName: icon)
                            Text(label)
                            Button { onDismiss(token) } label: {
                                Image(systemName: "xmark").imageScale(.small)
                            }
                            .buttonStyle(.plain)
                            .help("Keep “\(token.text)” as text")
                        }
                        .font(.callout)
                        .foregroundStyle(tint)
                        .padding(.horizontal, 9)
                        .padding(.vertical, 3)
                        .background(tint.opacity(0.14), in: .capsule)
                    }
                }
            }
        }
    }

    private func describe(_ token: ParsedToken, _ parsed: ParsedTask) -> (String, String) {
        switch token.kind {
        case .date: ("calendar", scheduleLabel(due: parsed.start, time: nil, repeat: nil, today: model.today) ?? token.text)
        case .deadline: ("hourglass", "Due " + (scheduleLabel(due: parsed.deadline, time: nil, repeat: nil, today: model.today) ?? token.text))
        case .time: ("clock", parsed.time.map(clockLabel) ?? token.text)
        case .repeat: ("repeat", parsed.repeat.flatMap(repeatLabel) ?? token.text)
        case .someday: ("cloud", "Someday")
        case .priority: ("flag.fill", "\(priorityName(parsed.priority)) priority")
        case .tag: ("number", token.text.trimmingCharacters(in: CharacterSet(charactersIn: "#")).lowercased())
        }
    }
}
