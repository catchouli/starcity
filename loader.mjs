// Trunk initializer: drives the Win95-style loading dialog in index.html while
// the wasm downloads. The dialog itself is removed by the app once it starts.

const SEGMENTS = 24;

export default function initializer() {
  const bar = () => document.getElementById("loading-bar");
  // The app may already have removed the dialog by the time the last callback runs.
  const say = (text) => {
    const el = document.getElementById("loading-label");
    if (el) el.textContent = text;
  };

  const setProgress = (fraction) => {
    const el = bar();
    if (!el) return;
    const filled = Math.round(Math.min(Math.max(fraction, 0), 1) * SEGMENTS);
    if (el.childElementCount !== filled) {
      el.replaceChildren(...Array.from({ length: filled }, () => document.createElement("i")));
    }
  };

  return {
    onStart: () => setProgress(0),
    onProgress: ({ current, total }) => {
      // `total` comes from Content-Length, which is the compressed size when the
      // server gzips (as GitHub Pages does), so `current` can run past it.
      const mb = (current / 1e6).toFixed(1);
      if (total && current <= total) {
        setProgress(current / total);
        say(`Downloading… ${mb} of ${(total / 1e6).toFixed(1)} MB`);
      } else {
        setProgress(0.95);
        say(`Downloading… ${mb} MB`);
      }
    },
    onComplete: () => {
      setProgress(1);
      say("Starting…");
    },
    onFailure: (error) => {
      say(`Failed to load: ${error}`);
    },
  };
}
