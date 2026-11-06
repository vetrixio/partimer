// Typed wrappers around the Rust commands (src-tauri/src/commands.rs).
// Types mirror crates/partimer-core/src/views.rs (camelCase JSON).

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Id = string;

export type TimerStatus = "idle" | "running" | "paused";

export interface Draft {
  title: string;
  content: string;
  /** Changes whenever the backend replaces the draft: rebuild the inputs. */
  generation: number;
}

export interface EntryView {
  id: Id;
  projectId: Id;
  projectName: string;
  clientName: string;
  color: number;
  startHhmm: string;
  endHhmm: string | null;
  /** Local start date, `YYYY-MM-DD`. */
  date: string;
  title: string;
  content: string;
  seconds: number;
  pausedMinutes: number;
  /** `idle` once the entry has ended. */
  status: TimerStatus;
}

export interface ProjectOption {
  id: Id;
  name: string;
  color: number;
}

export interface ProjectGroup {
  clientId: Id;
  clientName: string;
  projects: ProjectOption[];
}

export type NoticeKind =
  | "restored"
  | "corruptNoBackup"
  | "saveError"
  | "editedElsewhere"
  | "unreadable"
  | "newerVersion"
  | "info";

export interface Notice {
  id: number;
  kind: NoticeKind;
  message: string;
  dismissable: boolean;
}

export interface PanelState {
  groups: ProjectGroup[];
  selectedProjectId: Id | null;
  status: TimerStatus;
  running: EntryView | null;
  elapsedSeconds: number;
  draft: Draft;
  draftSaved: boolean;
  today: EntryView[];
  todayTotalSeconds: number;
  notices: Notice[];
  longRunningPrompt: boolean;
}

export interface RecordsQuery {
  year: number;
  month: number;
  clientId: Id | null;
  projectId: Id | null;
}

export interface ProjectTotalView {
  projectId: Id;
  label: string;
  color: number;
  minutes: number;
  hm: string;
  hours: string;
}

export interface DayView {
  date: string;
  /** 「週一」…「週日」 */
  weekday: string;
  entries: EntryView[];
}

export interface RecordsView {
  /** The query actually applied: filters pointing at deleted items are reset to null. */
  query: RecordsQuery;
  /** `[clientId, clientName]` pairs. */
  clients: [Id, string][];
  projects: ProjectOption[];
  totalMinutes: number;
  totalHm: string;
  totalHours: string;
  perProject: ProjectTotalView[];
  days: DayView[];
}

export interface EntryForm {
  projectId: Id;
  /** Local `YYYY-MM-DDTHH:MM`. */
  startLocal: string;
  endLocal: string | null;
  stillRunning: boolean;
  pausedMinutes: number;
  title: string;
  content: string;
}

export interface ProjectView {
  id: Id;
  name: string;
  color: number;
  archived: boolean;
  entryCount: number;
}

export interface ClientView {
  id: Id;
  name: string;
  archived: boolean;
  projects: ProjectView[];
}

export interface CatalogView {
  clients: ClientView[];
}

export type ItemKind = "client" | "project";

export interface DeletionImpact {
  entryCount: number;
  totalMinutes: number;
  includesRunning: boolean;
}

export type Reminder =
  | { kind: "longRunning"; projectName: string; hours: number }
  | { kind: "hourly"; projectName: string; hours: number };

export interface WidgetSettings {
  /** null = platform default (only in stored settings; SettingsView always resolves it). */
  enabled: boolean | null;
  opacity: number;
  showWhenIdle: boolean;
  x: number | null;
  y: number | null;
}

export interface ShortcutStatus {
  accelerator: string;
  registered: boolean;
  wayland: boolean;
}

export interface SettingsView {
  autostartEnabled: boolean;
  autostartError: string | null;
  autostartHelp: string;
  widget: WidgetSettings;
  hourlyReminder: boolean;
  shortcut: ShortcutStatus;
  shortcutHelp: string | null;
  dataDir: string;
}

export type WindowKind = "records" | "settings";
export type PanelOrigin = "widget" | "tray" | "shortcut";

export const api = {
  // panel
  getPanelState: () => invoke<PanelState>("get_panel_state"),
  /** `generation`: the draft the fields were built from; the backend ignores stale syncs. */
  setDraft: (title: string, content: string, generation: number) =>
    invoke<void>("set_draft", { title, content, generation }),
  start: (projectId: Id, title: string, content: string) =>
    invoke<PanelState>("start", { projectId, title, content }),
  stop: (title: string, content: string) => invoke<PanelState>("stop", { title, content }),
  saveDraft: (title: string, content: string) =>
    invoke<PanelState>("save_draft", { title, content }),
  pause: () => invoke<PanelState>("pause"),
  resume: () => invoke<PanelState>("resume"),
  selectProject: (projectId: Id, title: string, content: string) =>
    invoke<PanelState>("select_project", { projectId, title, content }),
  addProject: (clientName: string, projectName: string, title: string, content: string) =>
    invoke<PanelState>("add_project", { clientName, projectName, title, content }),
  stopAt: (title: string, content: string, endLocal: string) =>
    invoke<PanelState>("stop_at", { title, content, endLocal }),
  snoozeLongRunning: () => invoke<PanelState>("snooze_long_running"),
  dismissNotice: (id: number) => invoke<PanelState>("dismiss_notice", { id }),

  // records
  getRecords: (query: RecordsQuery) => invoke<RecordsView>("get_records", { query }),
  /** The entry the window was opened for, once; call after registering the open-entry listener. */
  takePendingEntry: () => invoke<Id | null>("take_pending_entry"),
  getEntryForm: (id: Id | null) => invoke<EntryForm>("get_entry_form", { id }),
  saveEntry: (id: Id | null, form: EntryForm) => invoke<void>("save_entry", { id, form }),
  deleteEntry: (id: Id) => invoke<void>("delete_entry", { id }),
  /** Opens a save dialog; false when the user cancels. */
  exportCsv: (query: RecordsQuery) => invoke<boolean>("export_csv", { query }),

  // catalog
  getCatalog: () => invoke<CatalogView>("get_catalog"),
  renameClient: (id: Id, name: string) => invoke<void>("rename_client", { id, name }),
  renameProject: (id: Id, name: string) => invoke<void>("rename_project", { id, name }),
  setProjectColor: (id: Id, color: number) => invoke<void>("set_project_color", { id, color }),
  setArchived: (kind: ItemKind, id: Id, archived: boolean) =>
    invoke<void>("set_archived", { kind, id, archived }),
  deletionImpact: (kind: ItemKind, id: Id) =>
    invoke<DeletionImpact>("deletion_impact", { kind, id }),
  deleteItem: (kind: ItemKind, id: Id) => invoke<void>("delete_item", { kind, id }),

  // settings
  getSettings: () => invoke<SettingsView>("get_settings"),
  setAutostart: (enabled: boolean) => invoke<SettingsView>("set_autostart", { enabled }),
  setWidgetSettings: (widget: WidgetSettings) =>
    invoke<SettingsView>("set_widget_settings", { widget }),
  widgetMoved: (x: number, y: number) => invoke<void>("widget_moved", { x, y }),
  setHourlyReminder: (enabled: boolean) =>
    invoke<SettingsView>("set_hourly_reminder", { enabled }),
  openDataFolder: () => invoke<void>("open_data_folder"),

  // windows
  openWindow: (kind: WindowKind, entryId: Id | null = null) =>
    invoke<void>("open_window", { kind, entryId }),
  togglePanel: (origin: PanelOrigin) => invoke<void>("toggle_panel", { origin }),
  quitApp: () => invoke<void>("quit_app"),
};

/** Data changed somewhere: refetch what this window shows. */
export function onStateChanged(handler: () => void): Promise<UnlistenFn> {
  return listen("state-changed", () => handler());
}

/** Records window only: open this entry in the editor. */
export function onOpenEntry(handler: (entryId: Id | null) => void): Promise<UnlistenFn> {
  return listen<Id | null>("open-entry", (e) => handler(e.payload));
}
