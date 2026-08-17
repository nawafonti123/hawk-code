const status = document.querySelector("#status");
const selector = document.querySelector("#selector");
const text = document.querySelector("#text");

const send = async (message) => {
  status.textContent = "Working…";
  const response = await chrome.runtime.sendMessage(message);
  if (!response?.ok)
    throw new Error(response?.error || "Browser Bridge request failed.");
  return response.result;
};

document.querySelector("#capture").addEventListener("click", async () => {
  try {
    const snapshot = await send({ type: "CAPTURE" });
    status.textContent = `Captured ${snapshot.interactive.length} interactive elements from ${snapshot.title || "the page"}.`;
  } catch (error) {
    status.textContent = error.message;
  }
});
document.querySelector("#click").addEventListener("click", async () => {
  try {
    await send({ type: "CLICK", selector: selector.value.trim() });
    status.textContent = "Clicked the selected element.";
  } catch (error) {
    status.textContent = error.message;
  }
});
document.querySelector("#type").addEventListener("click", async () => {
  try {
    await send({
      type: "TYPE",
      selector: selector.value.trim(),
      text: text.value,
    });
    status.textContent = "Entered text into the selected field.";
  } catch (error) {
    status.textContent = error.message;
  }
});
document.querySelector("#export").addEventListener("click", async () => {
  const { latestSnapshot } = await chrome.storage.local.get("latestSnapshot");
  if (!latestSnapshot) {
    status.textContent = "Capture a page first.";
    return;
  }
  const url = URL.createObjectURL(
    new Blob([JSON.stringify(latestSnapshot, null, 2)], {
      type: "application/json",
    }),
  );
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = "hawk-browser-snapshot.json";
  anchor.click();
  URL.revokeObjectURL(url);
  status.textContent = "Snapshot exported.";
});
