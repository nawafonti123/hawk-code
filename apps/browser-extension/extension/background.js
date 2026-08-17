const runInActiveTab = async (func, args = []) => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  if (!tab?.id) throw new Error("No active browser tab is available.");
  if (!tab.url?.startsWith("http://") && !tab.url?.startsWith("https://")) {
    throw new Error("Open a normal HTTP or HTTPS page first.");
  }
  const [result] = await chrome.scripting.executeScript({
    target: { tabId: tab.id },
    func,
    args,
  });
  return result?.result;
};

const capturePage = () => {
  const visible = (element) => {
    const rect = element.getBoundingClientRect();
    const style = getComputedStyle(element);
    return (
      rect.width > 0 &&
      rect.height > 0 &&
      style.visibility !== "hidden" &&
      style.display !== "none"
    );
  };
  const clean = (value) =>
    (value ?? "").replace(/\s+/g, " ").trim().slice(0, 500);
  const interactive = [
    ...document.querySelectorAll(
      "a,button,input,textarea,select,[role='button'],[tabindex]",
    ),
  ]
    .filter(visible)
    .slice(0, 250)
    .map((element, index) => ({
      index,
      tag: element.tagName.toLowerCase(),
      role: element.getAttribute("role"),
      type: element.getAttribute("type"),
      name: clean(
        element.getAttribute("aria-label") ||
          element.textContent ||
          element.getAttribute("placeholder"),
      ),
      id: element.id || null,
    }));
  return {
    capturedAt: new Date().toISOString(),
    url: location.href,
    title: document.title,
    selectedText: clean(getSelection()?.toString()),
    headings: [...document.querySelectorAll("h1,h2,h3")]
      .filter(visible)
      .slice(0, 80)
      .map((node) => clean(node.textContent)),
    interactive,
    textExcerpt: clean(document.body?.innerText).slice(0, 12000),
  };
};

const clickSelector = (selector) => {
  const element = document.querySelector(selector);
  if (!(element instanceof HTMLElement))
    throw new Error("Selector did not match an interactive element.");
  element.scrollIntoView({ block: "center", behavior: "smooth" });
  element.click();
  return true;
};

const typeSelector = (selector, text) => {
  const element = document.querySelector(selector);
  if (!(
    element instanceof HTMLInputElement ||
    element instanceof HTMLTextAreaElement
  ))
    throw new Error("Selector did not match an input or textarea.");
  element.focus();
  const setter = Object.getOwnPropertyDescriptor(
    Object.getPrototypeOf(element),
    "value",
  )?.set;
  setter?.call(element, text);
  element.dispatchEvent(new Event("input", { bubbles: true }));
  element.dispatchEvent(new Event("change", { bubbles: true }));
  return true;
};

chrome.runtime.onMessage.addListener((message, _sender, sendResponse) => {
  const execute = async () => {
    if (message.type === "CAPTURE") {
      const snapshot = await runInActiveTab(capturePage);
      await chrome.storage.local.set({ latestSnapshot: snapshot });
      return snapshot;
    }
    if (message.type === "CLICK")
      return runInActiveTab(clickSelector, [message.selector]);
    if (message.type === "TYPE")
      return runInActiveTab(typeSelector, [message.selector, message.text]);
    throw new Error("Unsupported HAWK Browser Bridge action.");
  };
  void execute()
    .then((result) => sendResponse({ ok: true, result }))
    .catch((error) =>
      sendResponse({
        ok: false,
        error: error instanceof Error ? error.message : String(error),
      }),
    );
  return true;
});
