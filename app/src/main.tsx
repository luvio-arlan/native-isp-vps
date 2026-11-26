import React from "react";
import ReactDOM from "react-dom/client";
import "./index.css";
import { CockpitApp } from "./views/CockpitApp";
import { PopoverHud } from "./views/PopoverHud";

const params = new URLSearchParams(window.location.search);
const isPopover = params.get("view") === "popover";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>{isPopover ? <PopoverHud /> : <CockpitApp />}</React.StrictMode>
);
