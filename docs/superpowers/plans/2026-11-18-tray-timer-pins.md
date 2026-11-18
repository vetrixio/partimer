# Tray Menu Timer Controls and Project Pinning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Start, pause, resume, stop and switch the timer from the system tray menu, listing only pinned projects (at most 5).

**Architecture:**
- `partimer-core` gains three things:
  - a `pinned` flag on `Project`, with catalog rules for it;
  - a pure "tray menu model" computed from `Data`;
  - `Session` methods for tray actions that decide from the live state.
- `src-tauri/src/tray.rs` turns the model into a muda menu and replaces it only when the model changes. The ticker calls it every second, and tray actions call it right away.
- Tray clicks dispatch on the item id plus the live state. A switch confirms first, with a native dialog shown on a spawned thread.
- The settings window gets a pin toggle and an 「已釘選 N／5」 counter.

**Tech Stack:** Rust 2024 (workspace: `crates/partimer-core`, `src-tauri`), Tauri 2.12 + muda 0.20 menus, tauri-plugin-dialog 2.8.4, Svelte 5 + TypeScript.

**Spec:** `docs/superpowers/specs/2026-11-18-tray-timer-pins-design.md` (the binding authority; "§" below refers to it).

## Global Constraints

- Platforms: same menu on macOS, Windows, Ubuntu (§2).
- Data: `Project.pinned: bool`, JSON key `pinned`, `#[serde(default)]`. `DATA_VERSION` stays `1` (§3.1).
- An **effective pin** is: `pinned` is true, the project is not archived, and its client is not archived (§3.2).
- `PIN_LIMIT = 5`, counted over effective pins.
- Exact copy (Traditional Chinese, verbatim):
  - Error when pinning past the limit: `最多釘選 5 個，請先取消一個`
  - Error when pinning an archived project (or a project of an archived client): `已封存的專案不能釘選`
  - Pinned project item: `▶ {client}・{project}`
  - Disabled hint when nothing is pinned: `到設定釘選專案，就能從這裡開始計時`
  - Disabled status line: `計時中：{client}・{project}` / `暫停中：{client}・{project}`
  - Action items: `暫停` / `繼續` / `停止計時`
  - Disabled section header: `改記其他專案：`
  - Existing items unchanged: `打開面板` / `紀錄` / `設定` / `結束` (quits the app)
  - Switch confirm dialog:
    - title `打工人計時器`
    - text `結束目前這筆，改記另一個專案？`
    - buttons `結束並切換` / `取消`
    - kind Warning
  - Settings counter: `已釘選 {n}／5`
- Order everywhere = `catalog::active_groups` order: clients by name, then projects by name (§4.4).
- Names are shown literally. In every menu label, escape `&` as `&&`; muda already escapes `_` on GTK (§4.3). Truncate each name part over 20 characters to 19 characters + `…`, so a whole label stays around 40 characters.
- Archiving a project unpins it. Archiving a client unpins all its projects. Unarchiving never re-pins (§3.3).
- Pin changes are not written to the activity log, same as the other catalog changes (§3.4).
- Tray actions never hold the session lock while a dialog is open. Every successful change calls `commands::changed(app)` (§5).
- Stop and switch text source: the session draft, or the running entry's stored text when `draft_conflict` is set. This is the same rule `flush_on_exit` uses (§5.3).
- A stale click changes nothing and shows no notification (§5.0). Real failures show a system notification (§5.5).
- Commits: English messages, no co-author or attribution lines.

## Review Focus

1. **A click on a stale menu after state changed elsewhere.** Example: the panel's STOP is pressed while the Ubuntu menu is open, then 「停止計時」 is clicked. Expected: nothing happens, with no error and no notification. Tested in Task 3 (`tray_stop_when_idle_is_noop`, `tray_pick_running_project_is_noop`).
2. **Unsaved panel text while the records window edited the same entry (draft conflict).** Expected: tray stop keeps the records-window text, not the panel text. Tested in Task 3 (`tray_stop_with_conflict_keeps_stored_text`).
3. **The switch dialog is open while the timer is stopped or switched elsewhere.** Expected: confirming does nothing. Tested in Task 3 (`tray_switch_after_state_changed_is_noop`).
4. **Data with an archived project still `pinned: true` (written by v0.1.5), or more than 5 effective pins (transfer-in).** Expected: no crash. The menu lists effective pins only, at most 5, and no new pin is allowed. Tested in Tasks 1 and 2.
5. **Project names with `&`, `_`, or more than 20 characters.** Expected: shown literally (`A&B` stays `A&B`), truncated with `…`. Tested in Tasks 2 and 4 (`truncate_long_names`, `escape_ampersand`).

---

### Task 1: Pinned flag and catalog rules

**Files:**
- Modify: `crates/partimer-core/src/model.rs` (`Project`)
- Modify: `crates/partimer-core/src/catalog.rs`
  - add `PIN_LIMIT`, the new errors, `set_project_pinned` and `effective_pins`;
  - make `set_project_archived` and `set_client_archived` unpin.
- Modify: `crates/partimer-core/src/views.rs` (`ProjectView.pinned`, `CatalogView.pin_limit`)
- Modify: `crates/partimer-core/src/session.rs`
  - `catalog()` fills the new fields;
  - add `set_project_pinned`.
- Modify: every `Project { … }` struct literal (find them with `grep -rn "Project {" crates src-tauri`) to add `pinned: false`.
- Test: `crates/partimer-core/src/catalog.rs` tests module; `crates/partimer-core/src/session/tests.rs`

**Interfaces:**
- Produces:
  - `model::Project { …, #[serde(default)] pub pinned: bool, … }`
  - `catalog::PIN_LIMIT: usize = 5`
  - `CatalogError::PinLimit` (Display `最多釘選 5 個，請先取消一個`)
  - `CatalogError::PinArchived` (Display `已封存的專案不能釘選`)
  - `catalog::set_project_pinned(data: &mut Data, id: &str, pinned: bool) -> Result<(), CatalogError>`:
    - `NotFound` for an unknown id;
    - unpinning always succeeds;
    - pinning an already pinned project is `Ok`;
    - otherwise `PinArchived` if not visible, then `PinLimit` if `effective_pins(data).len() >= PIN_LIMIT`.
  - `catalog::effective_pins(data: &Data) -> Vec<&Project>`: in `active_groups` order, all of them, not capped.
  - `ProjectView { …, pub pinned: bool }` (camelCase `pinned`)
  - `CatalogView { clients, pub pin_limit: usize }` (camelCase `pinLimit`)
  - `Session::set_project_pinned(&mut self, id: &str, pinned: bool, now: DateTime<Utc>) -> Result<(), String>`, via `catalog_change`.

- [ ] **Step 1: Write the failing tests** in `catalog.rs`:
  - `pin_limit_is_five`: pin 5 projects → `Ok`; the 6th → `Err(PinLimit)`. `PinLimit.to_string() == "最多釘選 5 個，請先取消一個"`.
  - `cannot_pin_archived`:
    - an archived project → `Err(PinArchived)`;
    - a project of an archived client → `Err(PinArchived)`;
    - the message is `已封存的專案不能釘選`.
  - `unpin_always_ok`: unpinning an archived project that is `pinned: true` → `Ok`, and the flag becomes false.
  - `archiving_unpins`:
    - `set_project_archived(p, true)` → `p.pinned == false`;
    - `set_client_archived(c, true)` → all of c's projects unpinned;
    - `set_*_archived(.., false)` leaves `pinned == false`.
  - `effective_pins_skip_archived_and_keep_order`: data has pinned `B/z`, `A/y`, archived-but-pinned `A/x`, and a pinned project under an archived client. Result ids are in the order `A/y`, `B/z`.
  - `limit_counts_effective_pins_only`: 5 pinned archived projects plus 0 effective → pinning a visible one is `Ok`.
  - `over_limit_data_blocks_new_pins`: 6 effective pins in data → pinning a 7th is `Err(PinLimit)`; unpinning one is `Ok`.
- [ ] **Step 2: Write the failing tests** in `model.rs` tests:
  - `missing_pinned_reads_false`: parse JSON without `pinned` → `false`.
  - `pinned_round_trips`: `to_json` writes `"pinned":true`.
- [ ] **Step 3: Write the failing test** in `session/tests.rs`:
  - `catalog_view_shows_pins`:
    - after `set_project_pinned("p1", true, now)`, `catalog()` shows p1 `pinned == true` and `pin_limit == 5`;
    - it is persisted on disk (`on_disk(&s)`).
- [ ] **Step 4: Run them and see them fail.** `cargo test -p partimer-core pin` fails with compile errors (no `pinned`).
- [ ] **Step 5: Implement** the interfaces above. Keep `set_project_pinned` beside the archive functions, and unpin inside the archive functions when `archived == true`.
- [ ] **Step 6: Run them and see them pass.** `cargo test -p partimer-core` is all green, and `cargo test --workspace` still builds, because `src-tauri` may construct `Project`.
- [ ] **Step 7: Commit.** `git commit -m "feat(core): pin projects with a limit of five, unpin on archive"`

---

### Task 2: Tray menu model

**Files:**
- Create: `crates/partimer-core/src/tray_menu.rs` (register as `pub mod tray_menu;` in `lib.rs`)
- Modify: `crates/partimer-core/src/session.rs` (`Session::tray_menu`)
- Test: unit tests inside `tray_menu.rs`; one session test in `session/tests.rs`

**Interfaces:**
- Consumes: `catalog::effective_pins`, `catalog::PIN_LIMIT`, `timer` running-entry lookup, `TimerStatus`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub struct PinItem { pub project_id: Id, pub label: String } // label = "{client}・{project}", parts truncated
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum TrayMenu {
      Idle { pins: Vec<PinItem> },                       // empty → hint row
      Active { paused: bool, current: String, others: Vec<PinItem> }, // current = "{client}・{project}"
  }
  pub fn tray_menu(data: &Data) -> TrayMenu;
  pub fn truncate_name(name: &str) -> String; // > 20 chars → first 19 chars + "…"
  impl Session<F> { pub fn tray_menu(&self) -> TrayMenu }
  ```
- Notes:
  - `pins` and `others` are effective pins in order, capped at `PIN_LIMIT` before the running project is removed from `others`. So `others` has at most 5 items, and at most 4 when the running project is pinned among the first 5.
  - `current` uses the running entry's project and client names even when archived or unpinned. A missing project or client name falls back to `?`.
  - Labels carry no `▶ ` prefix and no `&` escaping; those belong to the Tauri layer (Task 4).

- [ ] **Step 1: Write the failing tests** in `tray_menu.rs`:
  - `idle_lists_effective_pins_in_order` (Idle with the ids in order, labels `王小明・官網改版` style).
  - `idle_without_pins_is_empty`.
  - `idle_caps_at_five` (7 effective pins → 5).
  - `running_excludes_current_project`: running p1 (pinned) with pins p1, p2 → `Active{paused:false, current:"王小明・官網改版", others:[p2]}`.
  - `paused_sets_flag`.
  - `running_unpinned_archived_project_still_named`.
  - `truncate_long_names`:
    - a 21-character name → 19 characters + `…`;
    - a 20-character name is unchanged;
    - count chars, not bytes, with a Chinese name.
  - `archived_pinned_never_listed`.
- [ ] **Step 2: Write the failing test** `tray_menu_follows_session` in `session/tests.rs`: pin p2 → `Idle{pins:[p2]}`; after `start("p2", …)` → `Active{others: []}`.
- [ ] **Step 3: Run them and see them fail.** `cargo test -p partimer-core tray_menu`
- [ ] **Step 4: Implement** `tray_menu.rs` and `Session::tray_menu`.
- [ ] **Step 5: Run them and see them pass.** `cargo test -p partimer-core`
- [ ] **Step 6: Commit.** `git commit -m "feat(core): compute the tray menu model from pinned projects"`

---

### Task 3: Session tray actions

**Files:**
- Modify: `crates/partimer-core/src/session.rs`, or create `crates/partimer-core/src/session/tray.rs` as a child module like `session/moving.rs` (implementer's choice; prefer the child module).
- Test: `crates/partimer-core/src/session/tests/tray.rs` (registered like `tests/moving.rs`)

**Interfaces:**
- Consumes: the existing `select_project`, `start`, `stop`, `pause`, `resume`, `draft`, `draft_conflict`, `running()`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum TrayPick {
      Started,
      NoChange,                       // already timing this project
      ConfirmSwitch { running_entry: Id }, // caller asks, then calls tray_switch
  }
  impl<F: Fs> Session<F> {
      pub fn tray_pick(&mut self, project_id: &str, now: DateTime<Utc>) -> Result<TrayPick, String>;
      pub fn tray_switch(&mut self, project_id: &str, running_entry: &str, now: DateTime<Utc>) -> Result<bool, String>; // false = state changed, nothing done
      pub fn tray_pause(&mut self, now: DateTime<Utc>) -> Result<bool, String>;  // false = not running or already paused
      pub fn tray_resume(&mut self, now: DateTime<Utc>) -> Result<bool, String>; // false = not paused
      pub fn tray_stop(&mut self, now: DateTime<Utc>) -> Result<bool, String>;   // false = nothing running
  }
  ```
- Behaviour, from spec §5.0–§5.4:
  - `tray_pick` while idle does `select_project(id, draft.title, draft.content)`, then `start(id, draft.title, draft.content)` with the draft as it is after the select. It returns `Started`. A project that is not visible (archived or deleted) returns the existing `select_project` error string.
  - `tray_pick` while running the same project returns `NoChange`. While running another project it returns `ConfirmSwitch` and changes nothing.
  - `tray_switch` returns `Ok(false)` when nothing is running or the running entry id ≠ `running_entry`. Otherwise it calls `select_project(id, text…)` with the stop text source.
  - Stop text source (shared private helper): when `draft_conflict` is set, use the running entry's stored title and content; otherwise use `draft.title` / `draft.content`.
  - `tray_stop` uses the same source, then the existing `stop`.

- [ ] **Step 1: Write the failing tests** in `session/tests/tray.rs`:
  - `tray_pick_idle_starts_with_draft`: idle, selected p1, `set_draft("寫好的標題", "內容", gen)` → `tray_pick("p1")` = `Started`; the running entry has that title and content.
  - `tray_pick_other_project_uses_its_last_title`: idle, selected p1, draft content "x" → `tray_pick("p2")` → running p2 with title `配色` and content `x`. This matches the panel switch rule.
  - `tray_pick_running_project_is_noop` → `NoChange`, and `on_disk` is unchanged.
  - `tray_pick_while_running_other_asks` → `ConfirmSwitch{running_entry: <id>}`, and still running p1.
  - `tray_pick_archived_project_errors` → `Err(...)`, nothing started.
  - `tray_switch_ends_current_with_draft_and_starts_next`: running p1 with draft text "面板文字" → `tray_switch("p2", id)` = `Ok(true)`. The old entry is ended with "面板文字", and p2 is running with title `配色` and empty content. The activity log has SWITCH then START.
  - `tray_switch_after_state_changed_is_noop`:
    - stopped meanwhile → `Ok(false)`;
    - a different running entry id → `Ok(false)`.
  - `tray_stop_uses_draft`: draft "未存的字" → the ended entry has it.
  - `tray_stop_with_conflict_keeps_stored_text`: running entry edited via `save_entry` while the draft differs (so `draft_conflict` is set) → `tray_stop` → the ended entry has the records-window text.
  - `tray_stop_when_idle_is_noop` → `Ok(false)`.
  - `tray_pause_resume_are_idempotent`:
    - pause → `true`, pause again → `false`;
    - resume → `true`, resume again → `false`;
    - while idle, both → `false`.
- [ ] **Step 2: Run them and see them fail.** `cargo test -p partimer-core tray_`
- [ ] **Step 3: Implement** the methods with the behaviour above. Reuse the existing methods; do not duplicate timer logic.
- [ ] **Step 4: Run them and see them pass.** `cargo test -p partimer-core`
- [ ] **Step 5: Commit.** `git commit -m "feat(core): tray actions that act on the live timer state"`

---

### Task 4: Tray menu in the app

**Files:**
- Modify: `src-tauri/src/tray.rs`
- Modify: `src-tauri/src/ticker.rs` (call the menu refresh in `tick`; make `notify` `pub(crate)`)
- Test: unit tests in `src-tauri/src/tray.rs`

**Interfaces:**
- Consumes:
  - `Session::tray_menu`, `TrayMenu`, `PinItem`;
  - `Session::tray_pick / tray_switch / tray_pause / tray_resume / tray_stop`, `TrayPick`;
  - `commands::changed`, `commands::session` (or `lock(&state.session)`);
  - `ticker::notify`.
- Produces:
  - Menu item ids: `pin:<project id>`, `pause`, `resume`, `stop`, plus the existing `panel`, `records`, `settings`, `quit` constants.
  - `fn menu_label(label: &str) -> String`: replaces `&` with `&&`.
  - `fn menu_spec(model: &TrayMenu) -> Vec<Row>`: a pure function, testable without Tauri. `Row` is a private enum: `Item { id: String, text: String, enabled: bool }`, or `Separator`. It lists the rows in §4.1 / §4.2 order, followed by the fixed tail (separator, 打開面板, 紀錄, 設定, separator, 結束).
    - The hint row and status/header rows are `enabled: false`, with ids `hint`, `status`, `others`.
    - Pinned rows are `▶ ` + `menu_label(label)`.
    - When `others` is empty, the 改記 section and its separator are omitted.
  - `pub fn refresh_menu(app: &AppHandle)`:
    - computes the model under the session lock, then drops the lock;
    - compares it to the last model stored in `TrayState` (new field `menu: Mutex<Option<TrayMenu>>`);
    - only on a difference, builds a `Menu` from `menu_spec` and calls `tray.set_menu(Some(menu))`.
  - Called from `ticker::tick` (each second, after `tray::update`), from `build` (initial menu), and after every tray action.
  - `on_menu_event` dispatch:
    - `pin:*` → `tray_pick`. On `ConfirmSwitch`, spawn a `std::thread` that shows the confirm dialog (Global Constraints copy) with `blocking_show`. On OK it calls `tray_switch` and then does the same success handling.
    - `pause` / `resume` / `stop` → the matching session method.
    - After an action: `Ok(true)` / `Started` → `commands::changed(app)`, then `refresh_menu(app)`. `Ok(false)` / `NoChange` → just `refresh_menu(app)`. `Err(msg)` → `ticker::notify(app, &msg)`, then `refresh_menu(app)`.
    - Never hold the session lock across the dialog.

- [ ] **Step 1: Write the failing tests** in `tray.rs` tests:
  - `escape_ampersand`: `menu_label("A&B_c") == "A&&B_c"`.
  - `idle_spec_rows`: `Idle{pins:[p1 "王小明・官網改版"]}` → first row `Item{id:"pin:p1", text:"▶ 王小明・官網改版", enabled:true}`, then the fixed tail ending in `結束`.
  - `idle_without_pins_shows_disabled_hint`: the first row is `Item{id:"hint", text:"到設定釘選專案，就能從這裡開始計時", enabled:false}`.
  - `running_spec_rows`: `Active{paused:false, current:"王小明・官網改版", others:[p2]}` → `計時中：王小明・官網改版` (disabled), `暫停`, `停止計時`, separator, `改記其他專案：` (disabled), `▶ 王小明・設計`, then the tail.
  - `paused_spec_shows_resume`: `暫停中：…` and `繼續` (id `resume`).
  - `running_without_others_omits_section`: no `others` row, and no double separator.
- [ ] **Step 2: Run them and see them fail.** `cargo test -p partimer tray`.
- [ ] **Step 3: Implement** `menu_label`, `menu_spec`, `refresh_menu`, the event dispatch, and the ticker hook. Keep `show_menu_on_left_click(false)` and the existing Linux comment.
- [ ] **Step 4: Run them and see them pass.** `cargo test --workspace` and `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] **Step 5: Smoke run (headless).** If `xvfb-run` is available, start the app with a seeded data dir that has 2 pinned projects. Confirm over D-Bus that the exported menu (`com.canonical.dbusmenu` `GetLayout`) contains `▶ ` items. Then activate a `pin:` item through dbusmenu `Event` (`clicked`) and check `data.json` gets a running entry. Record the result, or why it could not run, in the report.
- [ ] **Step 6: Commit.** `git commit -m "feat(app): start, pause, stop and switch from the tray menu"`

---

### Task 5: Pin toggle in the settings window

**Files:**
- Modify: `src-tauri/src/commands.rs` (new `set_project_pinned` command); `src-tauri/src/lib.rs` (register it)
- Modify: `src/lib/api.ts` (`ProjectView.pinned`, `CatalogView.pinLimit`, `setProjectPinned`)
- Modify: `src/settings/CatalogEditor.svelte` (toggle per project row)
- Modify: `src/settings/Settings.svelte` (counter beside the `<h2>客戶與專案</h2>` heading), or place it inside `CatalogEditor` if the heading lives there conceptually (implementer's choice).
- Test: `tests/frontend/` only if logic is extracted (e.g. a `pinnedCount(catalog)` helper in `src/lib/`); `npm run check`.

**Interfaces:**
- Consumes: `Session::set_project_pinned`, `CatalogView.pin_limit`, `ProjectView.pinned`.
- Produces:
  - Command `set_project_pinned(id: String, pinned: bool) -> Res<()>`, via the existing `mutate` helper, so it emits `state-changed`.
  - `api.setProjectPinned(id: Id, pinned: boolean) => invoke<void>("set_project_pinned", { id, pinned })`.
  - Toggle:
    - a `<button type="button" aria-pressed={p.pinned}>` with text `釘選` / `已釘選`;
    - disabled when the project or its client is archived;
    - uses the existing `guarded(p.id, …)` so errors show in that row's error slot.
  - Counter text: `已釘選 {n}／{catalog.pinLimit}`. Here n counts projects with `pinned`, not archived, and not under an archived client.

- [ ] **Step 1: Write the failing test** (if a helper is extracted) `tests/frontend/pins.test.ts`: `pinnedCount` ignores archived projects and projects of archived clients.
- [ ] **Step 2: Implement** the command, API and UI.
- [ ] **Step 3: Verify.**
  - `npm test`, `npm run check` (0 errors, 0 warnings), `npm run build`, and `cargo test --workspace` all pass.
  - Optional: mount `CatalogEditor` in a throwaway headless-Chromium harness with a mocked `api` to see the toggle and counter.
- [ ] **Step 4: Commit.** `git commit -m "feat(ui): pin projects in settings with an N/5 counter"`

---

### Task 6: Documentation

**Files:**
- Modify: `README.md`: pinning, and timing from the tray menu (all platforms; on Ubuntu the menu is the only way).
- Modify: `docs/superpowers/specs/2026-11-06-partimer-design.md`: the tray menu section lists the new rows and points to the new spec.
- Modify: `docs/verification-checklist.md`. Add items with one box per platform. Use the next free ids in the tray/common sections (check the existing numbering first). Cover:
  - Menu content in each state: idle with pins, idle without pins (disabled hint), running, paused, running with no other pins (section omitted).
  - From the menu: start, pause, resume, stop, and switch with the confirm (取消 changes nothing; 結束並切換 ends and starts). The panel and records window update immediately.
  - Unsaved panel text is written on menu stop and switch, with the panel open and with it hidden. With a draft conflict, the records-window text is kept. Text still in IME composition is not included (known limitation).
  - A stale menu: open the Ubuntu menu, press STOP in the panel (e.g. with the global shortcut), then click 停止計時. Nothing happens and there is no notification.
  - Project names with `&` and `_` are shown literally. A long name is truncated with `…`.
  - Settings:
    - pinning a 6th project shows `最多釘選 5 個，請先取消一個`;
    - archiving a pinned project removes it from the menu and lowers the counter;
    - unarchiving does not re-pin;
    - the counter `已釘選 N／5` follows.
- [ ] **Step 1: Write the docs** above.
- [ ] **Step 2: Commit.** `git commit -m "docs: tray menu timer controls and project pinning"`
