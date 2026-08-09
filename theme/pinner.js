(function () {
  function sitePrefix() {
    var base = document.querySelector("base");
    if (base && base.getAttribute("href")) {
      return base.getAttribute("href");
    }

    var path = window.location.pathname || "/";
    var idx = path.indexOf("/Pinner/");
    if (idx >= 0) {
      return path.slice(0, idx + "/Pinner/".length);
    }
    return "/";
  }

  function injectMenuLogo() {
    var title = document.querySelector(".menu-title");
    if (!title || title.dataset.pinnerLogoAttached === "1") {
      return;
    }

    var img = document.createElement("img");
    img.className = "pinner-top-logo";
    img.alt = "Pinner";
    img.src = sitePrefix() + "inc/pinner_logo_clean.png";

    title.prepend(img);
    title.dataset.pinnerLogoAttached = "1";
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", injectMenuLogo);
  } else {
    injectMenuLogo();
  }
})();
