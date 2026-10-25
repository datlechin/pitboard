// <pb-account-picker>: macOS's Open Link window, whose own controls stay drawn, since its Open
// loads claude.ai (plan section 7); the menu bar the app puts up while it is in front works.
import { LiveFigure } from "./figure.ts";

class Picker extends LiveFigure {
  protected render() {}
}

customElements.define("pb-account-picker", Picker);
