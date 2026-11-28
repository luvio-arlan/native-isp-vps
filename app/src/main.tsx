import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { CockpitApp } from "./views/CockpitApp";
import { PopoverHud } from "./views/PopoverHud";
import { readCachedTheme } from "./lib/theme";

// Apply the last-used skin before the first paint so the window never flashes the other theme
// while settings are still loading over IPC.
document.documentElement.setAttribute("data-theme", readCachedTheme());

const params = new URLSearchParams(window.location.search);
const isPopover = params.get("view") === "popover";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isPopover ? <PopoverHud /> : <CockpitApp />}</React.StrictMode>
);
