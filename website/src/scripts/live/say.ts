// The page's one status region, rendered by Base.astro: it speaks what a Copy button did and what a
// figure shows for a result, and nothing the app itself does not show.
export function say(words: string) {
  const status = document.getElementById("status");
  if (status) status.textContent = words;
}
