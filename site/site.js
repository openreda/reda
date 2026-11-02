// The screenshot theme switch: shows the dark or light screenshots, remembering the choice
// in this browser only. Without a choice the screenshots follow the colour scheme.
(function () {
  "use strict";
  var root = document.documentElement;
  var key = "reda-shots";
  root.classList.add("js");

  function stored() {
    try { return localStorage.getItem(key); } catch (e) { return null; }
  }

  function current() {
    var chosen = root.getAttribute("data-shots");
    if (chosen) return chosen;
    return window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches
      ? "light" : "dark";
  }

  function show(theme, remember) {
    root.setAttribute("data-shots", theme);
    if (remember) {
      try { localStorage.setItem(key, theme); } catch (e) { /* private browsing */ }
    }
    var buttons = document.querySelectorAll("[data-shots]");
    for (var i = 0; i < buttons.length; i++) {
      if (buttons[i] === root) continue;
      buttons[i].setAttribute("aria-pressed", String(buttons[i].getAttribute("data-shots") === theme));
    }
  }

  var saved = stored();
  if (saved === "dark" || saved === "light") root.setAttribute("data-shots", saved);

  document.addEventListener("DOMContentLoaded", function () {
    show(current(), false);
    document.addEventListener("click", function (event) {
      var button = event.target.closest && event.target.closest("button[data-shots]");
      if (button) show(button.getAttribute("data-shots"), true);
    });
  });
})();
