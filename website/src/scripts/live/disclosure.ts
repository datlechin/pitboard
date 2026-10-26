// WinUI's SettingsExpander as the WAI-ARIA disclosure pattern; its whole header toggles it.
export function disclosure(button: HTMLButtonElement, header: HTMLElement) {
  const items = document.getElementById(button.getAttribute("aria-controls")!)!;
  const toggle = () => {
    const open = button.getAttribute("aria-expanded") !== "true";
    button.setAttribute("aria-expanded", String(open));
    items.hidden = !open;
  };
  button.addEventListener("click", toggle);
  header.addEventListener("click", (e) => {
    if ((e.target as Element).closest("a, button, input, [role=combobox], [role=listbox]")) return;
    toggle();
  });
}
