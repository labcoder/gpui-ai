document.documentElement.dataset.enhanced = "true";

for (const button of document.querySelectorAll("[data-copy]")) {
  button.addEventListener("click", async () => {
    const code = button.closest(".code-panel")?.querySelector("code")?.textContent ?? "";
    await navigator.clipboard?.writeText(code);
    button.textContent = "Copied";
  });
}
