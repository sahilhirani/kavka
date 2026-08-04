/**
 * Copy to the clipboard, with the one fallback that matters.
 *
 * `navigator.clipboard` needs a secure context and a user gesture; both hold
 * inside the Tauri webview, but a denied permission rejects rather than
 * throwing synchronously, and a copy button that silently does nothing is
 * worse than no copy button. The caller gets a boolean and says so.
 */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    /* fall through to the legacy path */
  }
  try {
    const ta = document.createElement("textarea");
    ta.value = text;
    ta.setAttribute("readonly", "");
    // Off-screen but not display:none — a hidden node cannot be selected.
    ta.style.position = "fixed";
    ta.style.top = "-1000px";
    ta.style.opacity = "0";
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand("copy");
    document.body.removeChild(ta);
    return ok;
  } catch {
    return false;
  }
}
