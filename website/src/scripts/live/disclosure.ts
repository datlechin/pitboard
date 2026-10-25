// WinUI's SettingsExpander as the WAI-ARIA disclosure pattern: its button shows and hides the items
// it controls. The expander's whole header is one toggle, so a click on the header outside its own
// controls toggles it too.
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
