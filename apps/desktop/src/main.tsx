import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { startAppearance } from "./appearance";
import { installCrashCapture } from "./diagnostics";
import "./styles.css";
// Loaded AFTER styles.css so an append wins at equal specificity. One file
// per sweep agent, so four parallel restyles never touch the same lines.
// Each names its owner in its header.
import "./styles/jackdaw-data.css";
import "./styles/jackdaw-ops.css";
import "./styles/jackdaw-shell.css";

// Before the first render, so a throw in the very first render is covered.
// Writes nothing unless the user has turned diagnostics on — see
// `diagnostics.ts` and the About panel's Diagnostics section.
installCrashCapture();

// The inline script in index.html already stamped the attributes; this
// re-stamps from the same source and installs the OS listener, which is the
// part a pre-paint script cannot do. Before render, so the first frame never
// paints under one theme and re-lays out under another.
startAppearance();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
