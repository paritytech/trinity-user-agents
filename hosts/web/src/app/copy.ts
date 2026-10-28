/** What copying needs from the page, so it can be checked without a browser. */
export interface CopyEnvironment {
  clipboard?: Pick<Clipboard, "writeText">;
  /** The old selection-based copy, for a page where `navigator.clipboard` is missing. */
  execCopy?: (text: string) => boolean;
}

/**
 * Copy `text` to the clipboard from a user's click. True only when the browser
 * took it. The asynchronous clipboard is tried first; the selection fallback
 * covers a page that is not a secure context, which has no `navigator.clipboard`.
 */
export async function copyText(
  text: string,
  environment: CopyEnvironment,
): Promise<boolean> {
  if (environment.clipboard !== undefined) {
    try {
      await environment.clipboard.writeText(text);
      return true;
    } catch {
      // Refused, for example without focus. The fallback may still work.
    }
  }
  try {
    return environment.execCopy?.(text) ?? false;
  } catch {
    return false;
  }
}

/** `copyText` against this page: the real clipboard, and a hidden field for the fallback. */
export function copyInPage(text: string): Promise<boolean> {
  return copyText(text, {
    clipboard: navigator.clipboard,
    execCopy(value) {
      const field = document.createElement("textarea");
      field.value = value;
      field.setAttribute("readonly", "");
      field.style.cssText = "position:fixed;top:0;left:0;opacity:0";
      document.body.append(field);
      field.select();
      try {
        return document.execCommand("copy");
      } finally {
        field.remove();
      }
    },
  });
}
