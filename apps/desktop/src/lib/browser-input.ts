const LOCAL_ADDRESS = /^(?:localhost|(?:\d{1,3}\.){3}\d{1,3}|\[::1\])(?::\d{1,5})?(?:[/?#].*)?$/i;
const PUBLIC_DOMAIN = /^(?:(?:[a-z\d](?:[a-z\d-]{0,61}[a-z\d])?)\.)+[a-z]{2,}(?::\d{1,5})?(?:[/?#].*)?$/i;

export function resolveBrowserInput(rawInput: string): string | null {
  const input = rawInput.trim();
  if (!input) return null;

  if (/^[a-z][a-z\d+.-]*:\/\//i.test(input)) {
    try {
      const url = new URL(input);
      return ["http:", "https:"].includes(url.protocol) ? url.toString() : null;
    } catch {
      return null;
    }
  }

  if (LOCAL_ADDRESS.test(input) || PUBLIC_DOMAIN.test(input)) {
    try {
      const protocol = LOCAL_ADDRESS.test(input) ? "http" : "https";
      return new URL(`${protocol}://${input}`).toString();
    } catch {
      return null;
    }
  }

  return `https://www.google.com/search?q=${encodeURIComponent(input)}`;
}
