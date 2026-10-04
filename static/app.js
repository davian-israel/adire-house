// Small behaviours htmx doesn't cover: closing the artwork modal and the mobile filter drawer.
(function () {
  function closeModal() {
    var m = document.getElementById("modal");
    if (m) m.innerHTML = "";
  }
  document.addEventListener("click", function (e) {
    var t = e.target;
    if (t.closest && t.closest("[data-close-modal]") && (t.matches("[data-close-modal]"))) closeModal();
    var tog = t.closest && t.closest("[data-toggle-rail]");
    if (tog) {
      var rail = document.getElementById("filters");
      if (rail) rail.classList.toggle("open");
    }
  });
  document.addEventListener("keydown", function (e) {
    if (e.key === "Escape") closeModal();
  });
  // Clear the flash area on every new request so stale errors don't linger.
  document.addEventListener("htmx:beforeRequest", function () {
    var f = document.getElementById("flash");
    if (f) f.innerHTML = "";
  });
  // Keep pushed filter URLs clean: drop empty parameters from GET requests.
  document.addEventListener("htmx:configRequest", function (e) {
    if (e.detail.verb !== "get") return;
    var p = e.detail.parameters;
    if (p instanceof FormData) {
      Array.from(p.keys()).forEach(function (k) { if (p.get(k) === "") p.delete(k); });
    } else {
      Object.keys(p).forEach(function (k) { if (p[k] === "") delete p[k]; });
    }
  });
  // Focus the dialog when it opens.
  document.addEventListener("htmx:afterSwap", function (e) {
    if (e.detail.target && e.detail.target.id === "modal") {
      var c = e.detail.target.querySelector(".close");
      if (c) c.focus();
    }
  });
})();
