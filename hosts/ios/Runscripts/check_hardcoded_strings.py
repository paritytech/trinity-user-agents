#!/usr/bin/env python3
import json
import os
import re
import sys

SCAN_DIRS = ["polkadot-app", "NotificationServiceExtension", "Packages"]
SKIP_PATH = re.compile(r"(/\.build/|/Tests?/|Tests?\.swift$|/Debug[^/]*/|/Generated/)")
NON_SHIPPING_FLAGS = {"DEBUG", "TESTNET_FEATURE"}
DIRECTIVE = re.compile(r"#(if|elseif|else|endif)\b\s*(.*)")
PREVIEW_START = re.compile(r"^(#Preview\b|.*:\s*PreviewProvider\b)")
OPENER = (
    r"\b(Text|Button|Label|TextField|SecureField|Toggle|Section|Link|Menu|Picker)\(\s*"
    r"|\.(navigationTitle|accessibilityLabel|accessibilityHint|accessibilityValue)\(\s*"
    r"|(?<!case )\b(title|subtitle|text|placeholder|actionTitle|buttonTitle)\s*[:=]\s*"
    r"|setTitle\(\s*"
)
PLACEHOLDER = r'"(?P<index>\d+)"'
UI_TEXT = re.compile(r"(?:" + OPENER + r")" + PLACEHOLDER)
TRAILING_OPENER = re.compile(r"(?:" + OPENER + r")$")
LEADING_LITERAL = re.compile(r"^" + PLACEHOLDER)
ESCAPE = re.compile(r"\\u\{[0-9A-Fa-f]+\}|\\.")
LETTER = re.compile(r"[A-Za-z]")
SPECIAL = re.compile(r'//|/\*|"')
LITERAL_SPECIAL = re.compile(r'["\\]')
IGNORE_MARKER = re.compile(r"^//\s*hardcoded-string:ignore(\s|$)")


class Line:
    def __init__(self, source, closing):
        self.source = source
        self.position = 0
        self.code = ""
        self.texts = []
        self.comment = ""
        self.closing = closing
        self.read()

    def peek(self, token):
        return self.source.startswith(token, self.position)

    def read(self):
        while self.position < len(self.source):
            if self.closing:
                end = self.source.find(self.closing, self.position)
                self.position = len(self.source) if end < 0 else end + len(self.closing)
                self.closing = self.closing if end < 0 else None
            elif self.peek("//"):
                self.comment = self.source[self.position:]
                break
            elif self.peek("/*") or self.peek('"""'):
                self.closing = "*/" if self.peek("/*") else '"""'
                self.position += len(self.closing)
            elif self.peek('"'):
                self.code += f'"{len(self.texts)}"'
                self.texts.append(self.read_literal())
            else:
                special = SPECIAL.search(self.source, self.position)
                end = special.start() if special else len(self.source)
                self.code += self.source[self.position:end]
                self.position = end

    def read_literal(self):
        self.position += 1
        text = ""
        while self.position < len(self.source) and not self.peek('"'):
            escape = ESCAPE.match(self.source, self.position)
            if self.peek("\\("):
                self.skip_interpolation()
            elif escape:
                self.position = escape.end()
            else:
                special = LITERAL_SPECIAL.search(self.source, self.position + 1)
                end = special.start() if special else len(self.source)
                text += self.source[self.position:end]
                self.position = end
        self.position += 1
        return text

    def skip_interpolation(self):
        self.position += 2
        depth = 1
        while self.position < len(self.source) and depth:
            if self.peek('"'):
                self.read_literal()
                continue
            depth += {"(": 1, ")": -1}.get(self.source[self.position], 0)
            self.position += 1


def split_top_level(expression, operator):
    parts, depth, start = [], 0, 0
    for index, char in enumerate(expression):
        depth += {"(": 1, ")": -1}.get(char, 0)
        if depth == 0 and expression.startswith(operator, index):
            parts.append(expression[start:index])
            start = index + len(operator)
    return parts + [expression[start:]]


def is_wrapped(expression):
    depth = 0
    for index, char in enumerate(expression):
        depth += {"(": 1, ")": -1}.get(char, 0)
        if depth == 0:
            return index == len(expression) - 1
    return False


def strip_parentheses(expression):
    expression = expression.strip()
    while expression.startswith("(") and is_wrapped(expression):
        expression = expression[1:-1].strip()
    return expression


def is_non_shipping(condition, negated=False):
    condition = strip_parentheses(condition)
    alternatives = split_top_level(condition, "||")
    if len(alternatives) > 1:
        verdicts = [is_non_shipping(part, negated) for part in alternatives]
        return any(verdicts) if negated else all(verdicts)
    conjuncts = split_top_level(condition, "&&")
    if len(conjuncts) > 1:
        verdicts = [is_non_shipping(part, negated) for part in conjuncts]
        return all(verdicts) if negated else any(verdicts)
    if condition.startswith("!"):
        return is_non_shipping(condition[1:], not negated)
    return not negated and condition in NON_SHIPPING_FLAGS


def is_branch_non_shipping(condition, earlier_conditions):
    taken = condition is not None and is_non_shipping(condition)
    return taken or any(is_non_shipping(earlier, negated=True) for earlier in earlier_conditions)


class Scope:
    def __init__(self):
        self.branches = []
        self.preview_depth = None
        self.depth = 0

    def advance(self, code):
        directive = DIRECTIVE.match(code)
        if directive:
            self.enter(directive.group(1), directive.group(2))

        if self.preview_depth is None and PREVIEW_START.match(code):
            self.preview_depth = self.depth
        in_preview = self.preview_depth is not None
        self.depth += code.count("{") - code.count("}")
        if in_preview and self.depth <= self.preview_depth and "}" in code:
            self.preview_depth = None

        return not in_preview and not any(skipped for skipped, _ in self.branches)

    def enter(self, keyword, condition):
        if keyword == "if":
            self.branches.append((is_branch_non_shipping(condition, []), [condition]))
        elif keyword in ("elseif", "else") and self.branches:
            earlier = self.branches[-1][1]
            current = condition if keyword == "elseif" else None
            self.branches[-1] = (is_branch_non_shipping(current, earlier), earlier + [condition])
        elif keyword == "endif" and self.branches:
            self.branches.pop()


def is_hit(line, previous):
    def has_text(index):
        return bool(LETTER.search(line.texts[int(index)]))

    code = line.code.strip()
    if any(has_text(match.group("index")) for match in UI_TEXT.finditer(code)):
        return True
    leading = LEADING_LITERAL.match(code)
    return bool(leading and TRAILING_OPENER.search(previous) and has_text(leading.group("index")))


def scan(path):
    scope = Scope()
    previous = ""
    closing = None
    with open(path, encoding="utf-8", errors="ignore") as source:
        for number, raw in enumerate(source, 1):
            line = Line(raw.rstrip("\n"), closing)
            closing = line.closing
            code = line.code.strip()
            if not code:
                continue
            if not scope.advance(code):
                previous = ""
                continue
            if is_hit(line, previous) and not IGNORE_MARKER.match(line.comment):
                yield number, raw.strip()
            previous = code


def is_checked(path, root):
    relative = "/" + os.path.relpath(path, root)
    return path.endswith(".swift") and relative.split("/")[1] in SCAN_DIRS and not SKIP_PATH.search(relative)


def swift_files(root, paths):
    if paths:
        for path in paths:
            path = os.path.join(root, path)
            if os.path.isfile(path) and is_checked(path, root):
                yield path
        return
    for directory in SCAN_DIRS:
        for folder, _, files in os.walk(os.path.join(root, directory)):
            for name in sorted(files):
                path = os.path.join(folder, name)
                if is_checked(path, root):
                    yield path


def report(path, root, line, message, as_rdjsonl):
    if as_rdjsonl:
        location = {"path": os.path.relpath(path, root), "range": {"start": {"line": line}}}
        print(json.dumps({"message": message, "location": location, "severity": "ERROR"}))
    else:
        print(f"{os.path.abspath(path)}:{line}: warning: {message}")


def main():
    arguments = sys.argv[1:]
    as_rdjsonl = "--rdjsonl" in arguments
    arguments = [argument for argument in arguments if argument != "--rdjsonl"]
    root = arguments[0] if arguments else "."
    found = False
    for path in swift_files(root, arguments[1:]):
        try:
            hits = list(scan(path))
        except OSError as error:
            hits = []
            found = True
            print(f"Hardcoded UI string check could not read {path}: {error}", file=sys.stderr)
        for number, code in hits:
            found = True
            report(path, root, number, "Hardcoded UI string, move it to a string catalog and use String(localized:), "
                   f"or use Text(verbatim:) / // hardcoded-string:ignore if it must not be localized: {code}",
                   as_rdjsonl)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main())
