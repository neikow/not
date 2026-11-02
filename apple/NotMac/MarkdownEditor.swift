import AppKit
import NotCore
import SwiftUI

/// A plain-text markdown editor that styles the markdown in place (headings, emphasis, checklists),
/// using the core's `markdownSpans` so it renders like the Android app. Click a checkbox to tick it;
/// Return continues lists; ⌘B / ⌘I wrap the selection.
struct MarkdownEditor: NSViewRepresentable {
    @Binding var text: String
    /// Bumped by the owner to move keyboard focus here.
    var focusRequests = 0

    func makeCoordinator() -> Coordinator { Coordinator(self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.hasVerticalScroller = true
        scroll.drawsBackground = false
        // TextKit 1: drawing boxes needs glyph positions from the layout manager, and asking a
        // TextKit 2 view for it switches it over mid-flight.
        let editor = MarkdownTextView(usingTextLayoutManager: false)
        editor.frame = NSRect(origin: .zero, size: scroll.contentSize)
        editor.autoresizingMask = [.width]
        editor.isVerticallyResizable = true
        editor.isHorizontallyResizable = false
        editor.textContainer?.widthTracksTextView = true
        editor.minSize = .zero
        editor.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        editor.delegate = context.coordinator
        editor.isRichText = false
        editor.allowsUndo = true
        editor.drawsBackground = false
        editor.isAutomaticQuoteSubstitutionEnabled = false
        editor.isAutomaticDashSubstitutionEnabled = false
        editor.textContainerInset = NSSize(width: 28, height: 22)
        editor.typingAttributes = MarkdownStyler.base
        editor.string = text
        MarkdownStyler.apply(to: editor)
        scroll.documentView = editor
        context.coordinator.textView = editor
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let editor = context.coordinator.textView else { return }
        if editor.string != text {
            let selection = editor.selectedRange()
            editor.string = text
            MarkdownStyler.apply(to: editor)
            let length = (text as NSString).length
            editor.setSelectedRange(NSRange(location: min(selection.location, length), length: 0))
        }
        if context.coordinator.focusRequests != focusRequests {
            context.coordinator.focusRequests = focusRequests
            DispatchQueue.main.async { editor.window?.makeFirstResponder(editor) }
        }
    }

    @MainActor
    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: MarkdownEditor
        weak var textView: MarkdownTextView?
        var focusRequests: Int

        init(_ parent: MarkdownEditor) {
            self.parent = parent
            focusRequests = parent.focusRequests
        }

        func textDidChange(_ notification: Notification) {
            guard let textView else { return }
            MarkdownStyler.apply(to: textView)
            parent.text = textView.string
        }

        func textView(_ textView: NSTextView, doCommandBy selector: Selector) -> Bool {
            guard selector == #selector(NSResponder.insertNewline(_:)) else { return false }
            return (textView as? MarkdownTextView)?.continueList() ?? false
        }
    }
}

final class MarkdownTextView: NSTextView {
    /// Checklist boxes and bullets drawn over their hidden markup; set by `MarkdownStyler`.
    var marks: [ListMark] = [] {
        didSet { needsDisplay = true }
    }

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        for case let .box(at, checked) in marks where boxRect(at: at).insetBy(dx: -6, dy: -4).contains(point) {
            replace(NSRange(location: at + 1, length: 1), with: checked ? " " : "x")
            return
        }
        super.mouseDown(with: event)
    }

    override func drawBackground(in rect: NSRect) {
        super.drawBackground(in: rect)
        for mark in marks {
            switch mark {
            case let .box(at, checked):
                let box = boxRect(at: at)
                guard box.intersects(rect) else { continue }
                let path = NSBezierPath(roundedRect: box, xRadius: 3.5, yRadius: 3.5)
                if checked {
                    NSColor.controlAccentColor.setFill()
                    path.fill()
                    let check = NSBezierPath()
                    check.move(to: NSPoint(x: box.minX + box.width * 0.24, y: box.minY + box.height * 0.52))
                    check.line(to: NSPoint(x: box.minX + box.width * 0.43, y: box.minY + box.height * 0.70))
                    check.line(to: NSPoint(x: box.minX + box.width * 0.77, y: box.minY + box.height * 0.32))
                    check.lineWidth = 1.8
                    check.lineCapStyle = .round
                    check.lineJoinStyle = .round
                    NSColor.white.setStroke()
                    check.stroke()
                } else {
                    let inset = NSBezierPath(roundedRect: box.insetBy(dx: 0.75, dy: 0.75), xRadius: 3, yRadius: 3)
                    inset.lineWidth = 1.5
                    NSColor.tertiaryLabelColor.setStroke()
                    inset.stroke()
                }
            case let .dot(at):
                let center = markCenter(at: at)
                NSColor.controlAccentColor.setFill()
                NSBezierPath(ovalIn: NSRect(x: center.x - 2.5, y: center.y - 2.5, width: 5, height: 5)).fill()
            }
        }
    }

    /// The square a box is drawn in, centred on its `[ ]`.
    private func boxRect(at: Int) -> NSRect {
        let center = markCenter(at: at)
        let side: CGFloat = 14
        return NSRect(x: center.x - side / 2, y: center.y - side / 2, width: side, height: side)
    }

    /// Where a box or bullet is drawn: half the box markup's width in from [at], centred on the
    /// line's capital letters.
    private func markCenter(at: Int) -> NSPoint {
        guard let layout = layoutManager, at < (string as NSString).length else { return .zero }
        let glyph = layout.glyphIndexForCharacter(at: at)
        let line = layout.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
        let location = layout.location(forGlyphAt: glyph)
        let origin = textContainerOrigin
        let baseline = origin.y + line.minY + location.y
        return NSPoint(
            x: origin.x + line.minX + location.x + MarkdownStyler.markupWidth / 2,
            y: baseline - MarkdownStyler.bodyFont.capHeight / 2
        )
    }

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        guard window?.firstResponder === self, event.modifierFlags.intersection(.deviceIndependentFlagsMask) == .command
        else { return super.performKeyEquivalent(with: event) }
        switch event.charactersIgnoringModifiers {
        case "b": wrapSelection("**")
        case "i": wrapSelection("*")
        default: return super.performKeyEquivalent(with: event)
        }
        return true
    }

    /// Return inside a list item starts the next item; on an empty item it ends the list.
    func continueList() -> Bool {
        let ns = string as NSString
        let caret = selectedRange().location
        let lineRange = ns.lineRange(for: NSRange(location: caret, length: 0))
        let line = ns.substring(with: NSRange(location: lineRange.location, length: caret - lineRange.location))
        guard let match = line.firstMatch(of: #/^(\s*)([-*+] \[[ xX]\] |[-*+] |(\d+)\. )/#) else { return false }
        let marker = String(match.output.0)
        if line == marker {
            // Empty item: drop the marker instead of adding another.
            replace(NSRange(location: lineRange.location, length: (marker as NSString).length), with: "")
            return true
        }
        var next = String(match.output.1) + String(match.output.2)
        if let number = match.output.3.flatMap({ Int($0) }) {
            next = String(match.output.1) + "\(number + 1). "
        } else if next.contains("[") {
            next = String(match.output.1) + String(match.output.2.prefix(1)) + " [ ] "
        }
        insertText("\n" + next, replacementRange: selectedRange())
        return true
    }

    private func wrapSelection(_ marker: String) {
        let range = selectedRange()
        let selected = (string as NSString).substring(with: range)
        replace(range, with: marker + selected + marker)
        setSelectedRange(NSRange(location: range.location + (marker as NSString).length, length: range.length))
    }

    /// An undoable edit that goes through the delegate like typing.
    private func replace(_ range: NSRange, with text: String) {
        guard shouldChangeText(in: range, replacementString: text) else { return }
        textStorage?.replaceCharacters(in: range, with: text)
        didChangeText()
    }
}

/// A checklist box (`[ ]` / `[x]` at `at`) or `-` bullet the editor draws itself.
enum ListMark {
    case box(at: Int, checked: Bool)
    case dot(at: Int)

    static func from(_ spans: [MarkdownSpan]) -> [ListMark] {
        let boxStarts = Set(spans.compactMap { span -> Int? in
            if case .checkbox = span.style { Int(span.start) } else { nil }
        })
        return spans.compactMap { span in
            switch span.style {
            case let .checkbox(checked): .box(at: Int(span.start), checked: checked)
            // Bullets of checklist items are hidden, not drawn; numbers stay text.
            case .bullet where span.end - span.start == 1 && !boxStarts.contains(Int(span.start) + 2):
                .dot(at: Int(span.start))
            default: nil
            }
        }
    }
}

/// Turns core markdown spans into text attributes.
@MainActor
enum MarkdownStyler {
    /// Box markup is monospaced so ticking it doesn't move the text after it.
    static let markupFont = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
    static var markupWidth: CGFloat { ("[ ]" as NSString).size(withAttributes: [.font: markupFont]).width }
    static let bodySize: CGFloat = 15
    static var bodyFont: NSFont { .systemFont(ofSize: bodySize) }
    static var base: [NSAttributedString.Key: Any] {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineSpacing = 3
        paragraph.paragraphSpacing = 4
        return [.font: bodyFont, .foregroundColor: NSColor.labelColor, .paragraphStyle: paragraph]
    }

    static func apply(to textView: NSTextView) {
        guard let storage = textView.textStorage else { return }
        let full = NSRange(location: 0, length: storage.length)
        let spans = markdownSpans(text: storage.string).map {
            (range: NSRange(location: Int($0.start), length: Int($0.end - $0.start)), style: $0.style)
        }
        let marks = ListMark.from(markdownSpans(text: storage.string))
        storage.beginEditing()
        storage.setAttributes(base, range: full)
        if let title = titleRange(storage.string as NSString, spans: spans) {
            storage.addAttribute(.font, value: NSFont.systemFont(ofSize: 22, weight: .semibold), range: title)
        }
        // Fonts first (headings, code), then traits on top of them, then colors and decorations.
        for (range, style) in spans {
            switch style {
            case let .heading(level):
                let size: CGFloat = [26, 21, 17][Int(level) - 1]
                storage.addAttribute(.font, value: NSFont.systemFont(ofSize: size, weight: level == 3 ? .semibold : .bold), range: range)
            case .code:
                storage.addAttribute(.font, value: NSFont.monospacedSystemFont(ofSize: bodySize - 1, weight: .regular), range: range)
                storage.addAttribute(.backgroundColor, value: NSColor.quaternaryLabelColor.withAlphaComponent(0.12), range: range)
            default: break
            }
        }
        for (range, style) in spans {
            let trait: NSFontTraitMask? = switch style {
            case .bold: .boldFontMask
            case .italic, .quote: .italicFontMask
            default: nil
            }
            guard let trait else { continue }
            storage.enumerateAttribute(.font, in: range) { value, run, _ in
                guard let font = value as? NSFont else { return }
                storage.addAttribute(.font, value: NSFontManager.shared.convert(font, toHaveTrait: trait), range: run)
            }
        }
        for (range, style) in spans {
            switch style {
            case .syntax:
                storage.addAttribute(.foregroundColor, value: NSColor.tertiaryLabelColor, range: range)
            case .quote:
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: range)
            case .bullet:
                storage.addAttribute(.foregroundColor, value: NSColor.controlAccentColor, range: range)
            case .checkbox:
                storage.addAttribute(.cursor, value: NSCursor.pointingHand, range: range)
            case .done, .strike:
                storage.addAttribute(.strikethroughStyle, value: NSUnderlineStyle.single.rawValue, range: range)
                storage.addAttribute(.foregroundColor, value: NSColor.secondaryLabelColor, range: range)
            default: break
            }
        }
        // Hide the markup the editor draws over. A box's bullet and space collapse so boxes line up
        // with bullets; a bullet is kerned as wide as box markup so their text lines up too.
        for mark in marks {
            switch mark {
            case let .box(at, _):
                let bullet = NSRange(location: max(at - 2, 0), length: min(2, at))
                storage.addAttributes([.font: NSFont.systemFont(ofSize: 0.1), .foregroundColor: NSColor.clear], range: bullet)
                storage.addAttributes([.font: markupFont, .foregroundColor: NSColor.clear], range: NSRange(location: at, length: 3))
            case let .dot(at):
                let dash = ("-" as NSString).size(withAttributes: [.font: markupFont]).width
                storage.addAttributes(
                    [.font: markupFont, .foregroundColor: NSColor.clear, .kern: markupWidth - dash],
                    range: NSRange(location: at, length: 1)
                )
            }
        }
        storage.endEditing()
        textView.typingAttributes = base
        (textView as? MarkdownTextView)?.marks = marks
    }

    /// The first non-blank line, which titles the note, unless it is a heading, quote or list item.
    private static func titleRange(_ text: NSString, spans: [(range: NSRange, style: MarkdownStyle)]) -> NSRange? {
        var start = 0
        while start < text.length {
            let line = text.lineRange(for: NSRange(location: start, length: 0))
            let content = text.substring(with: line).trimmingCharacters(in: .newlines)
            if !content.trimmingCharacters(in: .whitespaces).isEmpty {
                let marked = spans.contains { span in
                    guard span.range.location == start else { return false }
                    return switch span.style {
                    case .heading, .bullet, .syntax: true
                    default: false
                    }
                }
                return marked ? nil : NSRange(location: start, length: (content as NSString).length)
            }
            start = NSMaxRange(line)
        }
        return nil
    }
}
