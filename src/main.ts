import "./styles.css";
import { mount, type Component } from "svelte";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import Panel from "./panel/Panel.svelte";
import Records from "./records/Records.svelte";
import Settings from "./settings/Settings.svelte";
import Widget from "./widget/Widget.svelte";

// One bundle for every window; the window label picks the component.
const views: Record<string, Component> = {
  panel: Panel,
  records: Records,
  settings: Settings,
  widget: Widget,
};

const label = getCurrentWebviewWindow().label;
const view = views[label];
const target = document.getElementById("app");
if (view && target) {
  mount(view, { target });
}
