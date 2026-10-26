// <pb-account-picker>: macOS's Open Link window, whose Open would load claude.ai, so only its menu
// bar works (plan section 7).
import { LiveFigure } from "./figure.ts";

class Picker extends LiveFigure {
  protected render() {}
}

customElements.define("pb-account-picker", Picker);
