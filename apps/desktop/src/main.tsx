import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { installCrashCapture } from "./diagnostics";
import "./styles.css";

// Before the first render, so a throw in the very first render is covered.
// Writes nothing unless the user has turned diagnostics on — see
// `diagnostics.ts` and the About panel's Diagnostics section.
installCrashCapture();

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
