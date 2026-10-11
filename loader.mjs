// Trunk initializer: drives the Win95-style loading dialog in index.html while
// the wasm downloads. The dialog itself is removed by the app once it starts.

const SEGMENTS = 24;

export default function initializer() {
  const bar = () => document.getElementById("loading-bar");
  const label = () => document.getElementById("loading-label");

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
        label().textContent = `Downloading… ${mb} of ${(total / 1e6).toFixed(1)} MB`;
      } else {
        setProgress(0.95);
        label().textContent = `Downloading… ${mb} MB`;
      }
    },
    onComplete: () => {
      setProgress(1);
      label().textContent = "Starting…";
    },
    onFailure: (error) => {
      label().textContent = `Failed to load: ${error}`;
    },
  };
}
