// The WAI-ARIA toolbar pattern: one Tab stop, which the button last used keeps.
export function toolbar(bar: HTMLElement, act: (button: HTMLButtonElement) => void) {
  const buttons = () => [...bar.querySelectorAll<HTMLButtonElement>("button")];
  const stop = (to: HTMLButtonElement, focus: boolean) => {
    for (const b of buttons()) b.tabIndex = b === to ? 0 : -1;
    if (focus) to.focus();
  };
  bar.addEventListener("keydown", (e) => {
    const all = buttons();
    const at = all.indexOf(e.target as HTMLButtonElement);
    if (at < 0) return;
    const to =
      e.key === "ArrowRight"
        ? (at + 1) % all.length
        : e.key === "ArrowLeft"
          ? (at - 1 + all.length) % all.length
          : e.key === "Home"
            ? 0
            : e.key === "End"
              ? all.length - 1
              : null;
    if (to === null) return;
    e.preventDefault();
    stop(all[to], true);
  });
  bar.addEventListener("click", (e) => {
    const button = (e.target as Element).closest("button");
    if (!button) return;
    stop(button, false);
    act(button);
  });
}
