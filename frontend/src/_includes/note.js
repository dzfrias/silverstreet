function containsOnlyChineseCharacters(str) {
  // Regular expression to match Chinese characters
  const chineseCharRegex = /^[\u4e00-\u9fff!.?\s]+$/;
  return chineseCharRegex.test(str);
}

const delay = (ms) => new Promise((res) => setTimeout(res, ms));

let voice;

function getVoice() {
  voice = window.speechSynthesis.getVoices().find((voice) => {
    return voice.name === "Yu-shu";
  });
}

getVoice();
if (speechSynthesis.onvoiceschanged !== undefined) {
  speechSynthesis.onvoiceschanged = getVoice;
}

let cache = new Map();
async function translate(en_text) {
  if (cache.has(en_text)) {
    console.log("translation cache hit");
    return cache.get(en_text);
  }
  const res = await fetch(
    "https://silverstreet-server.shuttleapp.rs/translate",
    {
      method: "POST",
      body: JSON.stringify({
        en_text: en_text,
      }),
      headers: { "Content-Type": "application/json" },
    },
  );
  const json = await res.json();
  cache.set(en_text, json.zh_text);
  return json.zh_text;
}

let translationBox = null;
document.addEventListener("selectionchange", async () => {
  if (translationBox) {
    document.body.removeChild(translationBox);
    translationBox = null;
  }
  const selection = document.getSelection().toString();
  if (containsOnlyChineseCharacters(selection)) {
    await delay(1000);
    const newSelection = document.getSelection();
    if (selection !== newSelection.toString()) {
      return;
    }

    // volumeOn is a global variable defined in base.html
    if (volumeOn) {
      const utterThis = new SpeechSynthesisUtterance(selection);
      utterThis.pitch = 1.0;
      utterThis.rate = 0.9;
      utterThis.voice = voice;
      window.speechSynthesis.speak(utterThis);
    }

    const translation = await translate(selection);
    translationBox = document.createElement("div");
    translationBox.classList.add("translation-box");
    translationBox.textContent = translation;
    translationBox.style.top =
      newSelection.anchorNode.parentElement.getBoundingClientRect().top +
      window.scrollY +
      "px";
    document.body.appendChild(translationBox);
  }
});
