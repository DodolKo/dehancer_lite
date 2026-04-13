const dropzone = document.getElementById("dropzone");
const fileInput = document.getElementById("fileInput");
const chooseBtn = document.getElementById("chooseBtn");
const pageStatus = document.getElementById("pageStatus");

let wasmBindings = null;
let ready = false;

function setStatus(text) {
  pageStatus.textContent = text;
}

async function loadFile(file) {
  if (!wasmBindings?.load_web_image) {
    throw new Error("WASM app is not ready yet");
  }

  const bytes = new Uint8Array(await file.arrayBuffer());
  wasmBindings.load_web_image(file.name || "image", bytes);
  setStatus(`Loaded ${file.name || "image"} (${Math.round(file.size / 1024)} KB)`);
}

function wireDropzone() {
  const setActive = (on) => dropzone.classList.toggle("is-active", on);

  ["dragenter", "dragover"].forEach((evt) => {
    dropzone.addEventListener(evt, (e) => {
      e.preventDefault();
      setActive(true);
    });
  });

  ["dragleave", "drop"].forEach((evt) => {
    dropzone.addEventListener(evt, () => setActive(false));
  });

  dropzone.addEventListener("drop", async (e) => {
    e.preventDefault();
    const file = e.dataTransfer?.files?.[0];
    if (!file) {
      return;
    }
    try {
      await loadFile(file);
    } catch (error) {
      setStatus(`Import failed: ${error}`);
    }
  });

  chooseBtn.addEventListener("click", () => fileInput.click());
  fileInput.addEventListener("change", async (e) => {
    const file = e.target.files?.[0];
    if (!file) {
      return;
    }
    try {
      await loadFile(file);
    } catch (error) {
      setStatus(`Import failed: ${error}`);
    } finally {
      fileInput.value = "";
    }
  });
}

function markReady(bindings) {
  if (ready) {
    return;
  }

  wasmBindings = bindings;
  if (!wasmBindings?.load_web_image) {
    setStatus("Startup failed: image loader export is missing.");
    return;
  }

  try {
    wireDropzone();
    ready = true;
    setStatus("WASM loaded. Starting WebGPU renderer...");
  } catch (error) {
    setStatus(`Startup failed: ${error}`);
    console.error(error);
  }
}

addEventListener(
  "TrunkApplicationStarted",
  () => markReady(window.dehancerLite),
  { once: true },
);

addEventListener("DehancerAppReady", () => {
  setStatus("Renderer ready. Use presets + effect modules in the panel.");
});

if (window.dehancerLite) {
  markReady(window.dehancerLite);
}
