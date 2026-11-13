# 資料轉移（轉出／轉入）與匯入 CSV Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 設定視窗新增「轉出資料／轉入資料」（完整、覆蓋式的換電腦轉移），紀錄視窗新增「匯入 CSV」（把本 app 匯出的 CSV 讀回空資料）。

**Architecture:** 所有判斷都放在 `partimer-core`：`transfer.rs`（驗證轉移檔）、`csv_import.rs`（解析 CSV、還原時間、套用到資料）、`storage::write_snapshot`（改動前另存），再由 `session/moving.rs` 裡的 `Session` 方法串起來（檢查狀態 → 另存 → 取代／加入 → 存檔 → 活動紀錄）。Tauri 層的 `src-tauri/src/moving.rs` 只負責開檔／存檔／確認對話框，流程和現有的 `export_csv` 一樣，整個在 Rust 裡跑完，前端只呼叫一個 command、顯示回傳的訊息。

**Tech Stack:** Rust 2024（`partimer-core`，chrono、serde_json）、Tauri 2.12 + `tauri-plugin-dialog` 2.8.4（Rust 端的 `blocking_pick_file(s)`、`blocking_save_file`、`message(..).blocking_show()`）、Svelte 5 + TypeScript。

**Spec:** `docs/superpowers/specs/2026-11-13-import-transfer-design.md`（主設計：`docs/superpowers/specs/2026-11-06-partimer-design.md`）

## Global Constraints

- 不加任何新套件（主設計 §2：清單外的新套件需另外詢問）。CSV 解析器自己寫。
- 「匯出」「匯入」只用在 CSV。轉移一律用「轉出資料」「轉入資料」「轉移檔」。程式內部的名稱也用 `transfer`，不用 export／import。
- 轉移檔預設檔名 `partimer-transfer-YYYY-MM-DD.json`（本機日期），內容和 `data.json` 同格式（`model::to_json`）。
- 另存檔：轉入前 `data.before-transfer-YYYYMMDD-HHMMSS.json`，匯入 CSV 前 `data.before-import-YYYYMMDD-HHMMSS.json`，放在資料夾根目錄，內容從**記憶體中的資料**寫出，不自動清理。
- 活動紀錄動作：`TRANSFER_OUT`、`TRANSFER_IN`、`IMPORT_CSV`，不記文字內容。
- 全有或全無：檢查都做完才改資料；另存失敗就中止，資料不變。
- 禁止存檔（`save_blocked`）時，轉入和匯入 CSV 都拒絕；轉出不受影響。
- 錯誤訊息一律中文，原始錯誤放括號（沿用 `commands::export_error` 格式）。
- 表頭固定為 `日期,客戶,專案,開始,結束,分鐘,小時,暫停分鐘,標題,內容`（`report::CSV_HEADER`）。
- commit message 用英文，不加 Claude 署名（使用者全域偏好）。

## Review Focus

1. **轉入時面板上還有打到一半的草稿或「別處編輯」衝突**：轉入後草稿要換成新資料的（標題 = 最後選的專案的上一筆標題，內容空白），衝突提示消失，不能把舊草稿存進新資料。→ Task 5 的 `transfer_in_resets_draft_and_conflict`。
2. **轉入時正在計時、而且已經跳出 12 小時提醒**：轉入後提醒狀態要重設，不能對已經不存在的那筆再提醒；也不能寫出 STOP。→ Task 5 的 `transfer_in_while_running_drops_entry_without_stop`。
3. **同一個 CSV 檔被選兩次，或兩個月份檔重疊**：只匯入一次，確認文字寫出略過幾筆。→ Task 4 的 `duplicate_rows_across_files_are_skipped`。
4. **在 Excel 開過再存的 CSV（沒有 BOM、LF 換行，或日期變成 `2026/10/6`）**：前兩種照樣匯入，後一種報出檔名＋行號，資料不變。→ Task 3 的 `accepts_no_bom_and_lf`、`rejects_slash_date_with_file_and_line`。
5. **對話框開著時，另一個視窗改了資料**（例如確認轉入前按了 START，或確認匯入 CSV 前新增了紀錄）：套用時要用當下的資料重新檢查，不能用準備時的快照。→ Task 5 的 `transfer_in_snapshots_current_memory`，Task 6 的 `import_csv_rechecks_entries_at_apply`。

---

## File Structure

| 檔案 | 動作 | 職責 |
|---|---|---|
| `crates/partimer-core/src/activity.rs` | 改 | 加三個 `Action` |
| `crates/partimer-core/src/storage.rs` | 改 | `SnapshotKind`、`Paths::before`、`write_snapshot` |
| `crates/partimer-core/src/transfer.rs` | 新 | 驗證轉移檔、統計筆數與日期範圍 |
| `crates/partimer-core/src/csv_import.rs` | 新 | CSV 解析、時間還原、去重、套用到 `Data` |
| `crates/partimer-core/src/session/moving.rs` | 新 | `Session` 的轉出／轉入／匯入方法（`impl` 區塊，可讀 `Session` 私有欄位） |
| `crates/partimer-core/src/session/tests/moving.rs` | 新 | 上面的 session 測試（`tests.rs` 的子模組，可用它的 helper） |
| `crates/partimer-core/src/lib.rs`、`session.rs`、`session/tests.rs` | 改 | 掛上新模組 |
| `src-tauri/src/moving.rs` | 新 | 三個 Tauri command 與對話框 |
| `src-tauri/src/commands.rs`、`windows.rs`、`lib.rs` | 改 | 開放 helper、`reposition_widget`、註冊 command |
| `src/lib/api.ts`、`src/settings/Settings.svelte`、`src/records/Records.svelte` | 改 | 按鈕與呼叫 |
| `README.md`、主設計 §9.4、`docs/verification-checklist.md` | 改 | 文件 |

---

### Task 1: 活動紀錄動作與改動前另存

**Files:**
- Modify: `crates/partimer-core/src/activity.rs:9-39`
- Modify: `crates/partimer-core/src/storage.rs`（`Paths` 105-131，新函式放在 `ensure_daily_backup` 後面）
- Test: 同檔的 `mod tests`

**Interfaces:**
- Produces:
  - `Action::TransferOut`、`Action::TransferIn`、`Action::ImportCsv`，輸出 `TRANSFER_OUT`、`TRANSFER_IN`、`IMPORT_CSV`
  - `pub enum SnapshotKind { Transfer, Import }`
  - `impl Paths { pub fn before(&self, kind: SnapshotKind, stamp: &str) -> PathBuf }` → `dir/data.before-transfer-<stamp>.json`／`dir/data.before-import-<stamp>.json`
  - `pub fn write_snapshot(fs: &dyn Fs, paths: &Paths, kind: SnapshotKind, data: &Data, zone: &dyn LocalZone, now: DateTime<Utc>) -> io::Result<PathBuf>`

- [ ] **Step 1: 寫失敗的測試**

```rust
// activity.rs tests
#[test]
fn transfer_and_import_actions_render_upper_snake() {
    assert_eq!(Action::TransferOut.as_str(), "TRANSFER_OUT");
    assert_eq!(Action::TransferIn.as_str(), "TRANSFER_IN");
    assert_eq!(Action::ImportCsv.as_str(), "IMPORT_CSV");
}

// storage.rs tests（Asia/Taipei，now = 2026-11-13T06:07:08Z → 本機 14:07:08）
#[test]
fn write_snapshot_names_by_kind_and_local_time() {
    // write_snapshot(.., SnapshotKind::Transfer, &data, ..) 回傳 /d/partimer/data.before-transfer-20261113-140708.json
    // 檔案內容 == model::to_json(&data)；Import 則為 data.before-import-20261113-140708.json
}
#[test]
fn write_snapshot_never_overwrites_an_existing_one() {
    // 預先放同名檔 → 回傳 ...-140708-1.json，原檔內容不變
}
#[test]
fn write_snapshot_reports_write_failure() {
    // fs.fail_write_syncs = true → Err，且不留下檔案
}
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test -p partimer-core -- activity:: storage::tests::write_snapshot`
Expected: 編譯失敗（`TransferOut`、`write_snapshot` 不存在）

- [ ] **Step 3: 實作**

`write_snapshot` 用和 `unique_corrupt_path` 一樣的方式挑檔名（stamp `%Y%m%d-%H%M%S`，接著 `-1`…`-100`），把 `unique_corrupt_path` 改成吃一個 `impl Fn(&str) -> PathBuf` 共用，不要複製一份迴圈。找不到可用檔名時回 `io::Error::other("找不到可用的檔名")`。用 `fs.write_sync` 寫入。

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test -p partimer-core`
Expected: 全部 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/partimer-core/src/activity.rs crates/partimer-core/src/storage.rs
git commit -m "feat(core): add transfer/import log actions and pre-change snapshots"
```

---

### Task 2: 轉移檔驗證（`transfer.rs`）

**Files:**
- Create: `crates/partimer-core/src/transfer.rs`
- Modify: `crates/partimer-core/src/lib.rs`（`pub mod transfer;`）

**Interfaces:**
- Produces:
  - `pub enum TransferError { NotTransferFile, NewerVersion, HasRunning }`，用 `thiserror`，`Display` 就是給使用者看的中文：
    - `NotTransferFile` → `這不是打工人計時器的轉移檔，或檔案已損壞`
    - `NewerVersion` → `這個轉移檔來自較新版的打工人計時器，請先更新 app`
    - `HasRunning` → `轉移檔裡有一筆還在計時，請在舊電腦按 STOP 後重新轉出`
  - `pub fn parse_transfer(bytes: &[u8]) -> Result<Data, TransferError>`
  - `pub struct DataCounts { pub clients: usize, pub projects: usize, pub entries: usize }`（`entries` 含 `quarantined_entries`，`clients`／`projects` 也各加上被隔離的數量）
  - `pub fn counts(data: &Data) -> DataCounts`
  - `pub fn is_empty(data: &Data) -> bool`（三種項目和三種隔離項目都沒有）
  - `pub fn date_range(data: &Data, zone: &dyn LocalZone) -> Option<(NaiveDate, NaiveDate)>`（`entries` 的本機開始日期最早與最晚）

- [ ] **Step 1: 寫失敗的測試**

```rust
#[test] fn round_trips_to_json_exactly()            // parse_transfer(&to_json(&d)) == d（d 含 extra、quarantined_*、settings.widget x/y）
#[test] fn rejects_empty_object()                    // b"{}" → Err(NotTransferFile)
#[test] fn rejects_missing_entries_array()           // {"version":1,"clients":[],"projects":[]} → NotTransferFile
#[test] fn rejects_non_numeric_version()             // "version":"1" → NotTransferFile
#[test] fn rejects_garbage()                         // b"not json" → NotTransferFile
#[test] fn rejects_newer_version()                   // version = DATA_VERSION + 1 → NewerVersion
#[test] fn rejects_running_or_paused_entry()         // 一筆 end: null → HasRunning
#[test] fn counts_include_quarantined()              // 2 entries + 1 quarantined entry → entries == 3
#[test] fn date_range_uses_local_dates()             // Asia/Taipei，2026-10-05T17:00Z 算 10-06
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test -p partimer-core transfer::`
Expected: 編譯失敗（模組不存在）

- [ ] **Step 3: 實作**

先用 `serde_json::from_slice::<Value>` 檢查最上層形狀（物件、`version` 是數字、`clients`／`projects`／`entries` 是陣列），再呼叫 `model::parse_data`。`parse_data` 回 `Err` 也算 `NotTransferFile`。

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test -p partimer-core transfer::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/partimer-core/src/transfer.rs crates/partimer-core/src/lib.rs
git commit -m "feat(core): validate transfer files"
```

---

### Task 3: CSV 解析（`csv_import.rs` 前半）

**Files:**
- Create: `crates/partimer-core/src/csv_import.rs`
- Modify: `crates/partimer-core/src/lib.rs`（`pub mod csv_import;`）、`crates/partimer-core/src/report.rs:9`（`CSV_HEADER` 改成 `pub(crate)`）

**Interfaces:**
- Produces:
  - `pub struct CsvRow { pub file: String, pub line: usize, pub date: NaiveDate, pub client: String, pub project: String, pub start: NaiveTime, pub end: NaiveTime, pub minutes: i64, pub hours: String, pub paused_minutes: i64, pub title: String, pub content: String }`。`line` 是這一筆開始的實際行號（表頭是第 1 行）。去重用的 key 是 `file`、`line` 以外的全部欄位（實作 `fn key(&self)`）。
  - `pub struct RowError { pub file: String, pub line: Option<usize>, pub problem: String }`，`Display` 為 `` `{file}` 第 {line} 行：{problem} ``；`line` 為 `None` 時是 `` `{file}`：{problem} ``。
  - `pub fn parse_file(file: &str, bytes: &[u8]) -> Result<Vec<CsvRow>, Vec<RowError>>`

problem 文字（spec 固定的照抄，其餘是本計畫定的）：

| 情況 | problem |
|---|---|
| 不是 UTF-8 | `檔案不是 UTF-8 編碼，可能被 Excel 另存過` |
| 表頭不符 | `第一行不是打工人計時器匯出的欄位名稱` |
| 引號沒關 | `引號沒有成對` |
| 欄數 | `欄位數是 {n}，應為 10` |
| 日期 | `日期『{v}』格式不對，應為 YYYY-MM-DD` |
| 開始／結束 | `開始時間『{v}』格式不對，應為 HH:MM`／`結束時間『{v}』格式不對，應為 HH:MM` |
| 分鐘／暫停分鐘 | `分鐘『{v}』應為 0 以上的整數`／`暫停分鐘『{v}』應為 0 以上的整數` |

- [ ] **Step 1: 寫失敗的測試**

測試的輸入一律用 `report::export_csv` 產生，或用和它同格式的字面字串。

```rust
#[test] fn parses_exported_csv()                    // export_csv 產生兩筆 → 兩個 CsvRow，欄位值相符
#[test] fn accepts_no_bom_and_lf()                   // 拿掉 BOM、CRLF 換成 LF → 結果相同
#[test] fn unquotes_commas_quotes_and_newlines()     // 標題 `a,"b"`、內容 "x\ny" 原樣還原；line 是該筆起始行
#[test] fn strips_injection_tab()                    // export 後的 "\t- 改 banner"、客戶 "=王" → "- 改 banner"、"=王"
#[test] fn keeps_tab_not_added_by_export()           // 有引號的 "\tabc"（TAB 後面不是 = + - @ TAB CR）→ 保留 "\tabc"
#[test] fn skips_blank_lines()
#[test] fn rejects_wrong_header()                    // RowError { line: None, problem: "第一行不是…" }
#[test] fn rejects_non_utf8()                        // Big5 位元組 → "檔案不是 UTF-8 編碼，可能被 Excel 另存過"
#[test] fn rejects_slash_date_with_file_and_line()   // 第 3 行日期 2026/10/6 → "`a.csv` 第 3 行：日期『2026/10/6』格式不對，應為 YYYY-MM-DD"
#[test] fn rejects_single_digit_time()               // "9:3" → 開始時間『9:3』格式不對
#[test] fn rejects_wrong_column_count()
#[test] fn rejects_negative_minutes()
#[test] fn rejects_unterminated_quote()
#[test] fn collects_all_row_errors()                 // 三列錯 → Vec 長度 3
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test -p partimer-core csv_import::`
Expected: 編譯失敗

- [ ] **Step 3: 實作 `parse_file`**

- 去掉開頭的 BOM `EF BB BF`（可有可無），`std::str::from_utf8` 失敗就回不是 UTF-8 的錯誤。
- 寫一個 RFC 4180 狀態機，切出 `(起始行號, Vec<(cell, quoted: bool)>)`。CR、LF、CRLF 都算換行；引號內的換行原樣保留。
- 還原防注入 TAB：只對 `quoted == true` 的欄位，如果它以 `\t` 開頭、而且第二個字元是 `= + - @ \t \r` 其中之一，就去掉開頭那一個 `\t`。這是 `report::csv_field` 的反向操作。
- 日期：長度必須是 10，再用 `%Y-%m-%d` 解析。時間：長度 5、第 3 個字元是 `:`，再用 `%H:%M` 解析，避免 chrono 接受一位數。分鐘數用 `i64` 解析，而且要 `>= 0`。
- 每一列都檢查完才回傳，錯誤一次收齊。

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test -p partimer-core csv_import::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/partimer-core/src/csv_import.rs crates/partimer-core/src/lib.rs crates/partimer-core/src/report.rs
git commit -m "feat(core): parse exported CSV files back into rows"
```

---

### Task 4: CSV 時間還原、去重、套用（`csv_import.rs` 後半）

**Files:**
- Modify: `crates/partimer-core/src/csv_import.rs`

**Interfaces:**
- Consumes: Task 3 的 `CsvRow`、`RowError`、`parse_file`；`catalog::add_project`；`LocalZone::from_local`
- Produces:
  - `pub struct ImportRow { pub client: String, pub project: String, pub start: DateTime<Utc>, pub end: DateTime<Utc>, pub paused_seconds: i64, pub title: String, pub content: String }`
  - `pub struct CsvImport { pub rows: Vec<ImportRow>, pub files: usize, pub skipped_duplicates: usize, pub first_date: NaiveDate, pub last_date: NaiveDate }`
  - `pub fn prepare(files: &[(String, Vec<u8>)], zone: &dyn LocalZone) -> Result<CsvImport, String>`
  - `pub fn new_catalog_counts(data: &Data, imp: &CsvImport) -> (usize, usize)`（會新增幾個客戶、幾個專案）
  - `pub fn apply(data: &mut Data, imp: CsvImport) -> usize`（回傳加入的筆數）
  - `pub(crate) const UNNAMED_CLIENT: &str = "未命名客戶"; pub(crate) const UNNAMED_PROJECT: &str = "未命名專案";`

- [ ] **Step 1: 寫失敗的測試**

```rust
#[test] fn resolves_same_day_entry()                 // 2026-10-06 09:30–11:00，分鐘 90，暫停 0（Asia/Taipei）→ start 01:30Z、end 03:00Z
#[test] fn resolves_overnight_entry()                // 23:00–01:30，分鐘 150 → end 是隔天
#[test] fn resolves_entry_longer_than_a_day()        // 09:00–10:00，分鐘 1500 → end 是隔天 10:00（k=1）
#[test] fn uses_pause_to_pick_the_day()              // 09:00–09:30，分鐘 30，暫停 1440 → end 是隔天
#[test] fn resolves_dst_repeated_hour()              // America/New_York 2026-11-01 01:30–01:50，分鐘 80 → 取較早的開始 + 較晚的結束，不報錯
#[test] fn rejects_dst_gap_start()                   // America/New_York 2026-03-08 02:30 → "開始時間 2026-03-08 02:30 在這台電腦的時區不存在（夏令時間）"
#[test] fn tolerates_two_minute_rounding()           // 推算差 2 分鐘 → OK
#[test] fn rejects_mismatch_over_two_minutes()       // 差 3 分鐘 → "結束時間和分鐘數對不起來（差 3 分鐘），檔案可能被改過"
#[test] fn clamps_pause_to_duration()                // 09:00–09:00，分鐘 0，暫停 1 → paused_seconds == 0
#[test] fn duplicate_rows_across_files_are_skipped() // 同一份 bytes 以兩個檔名傳入 → rows 只有一份，skipped_duplicates == 該檔筆數
#[test] fn error_list_caps_at_five()                 // 7 個錯 → 5 行加 "…還有 2 筆"，行與行用 "\n" 分隔
#[test] fn empty_selection_is_an_error()             // 只有表頭 → Err("選的檔案裡沒有任何紀錄")
#[test] fn apply_reuses_existing_names_including_archived() // 已有 王小明/官網改版（封存）→ 不新增，紀錄掛到它底下
#[test] fn apply_trims_names_and_fills_unnamed()     // " 王小明 " 對到 王小明；空白客戶 → 未命名客戶
#[test] fn apply_colors_follow_catalog_rule()        // 同一客戶新增 3 個專案 → color 0,1,2（沿用 add_project 的規則）
#[test] fn new_catalog_counts_matches_apply()
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test -p partimer-core csv_import::`
Expected: 新測試編譯失敗

- [ ] **Step 3: 實作**

`prepare`：對每個檔案呼叫 `parse_file`，再對每一列呼叫 `resolve`。所有 `RowError` 收齊後，有錯就回 `Err`，只列前 5 筆，後面加 `…還有 N 筆`。用 `CsvRow::key` 去重，保留第一次出現的那筆。最後 `rows` 依 `start` 排序。

時間還原的演算法（spec §5.3，這部分測試決定不了，所以寫在這裡）：

```text
span = minutes + paused_minutes                         // 分鐘
starts = from_local(date + start)                       // Single→[t]；Ambiguous(a,b)→[a,b]；None→錯誤（DST gap）
for k in 0..=(span / 1440 + 1):
    ends = from_local((date + k days) + end)            // None 的跳過
    for s in starts, e in ends where e >= s:
        diff = |(e - s).num_minutes() - span|
        取 diff 最小的 (s, e)；同分時取 k 較小、s 較早的
沒有任何組合，或 best diff > 2 → 「結束時間和分鐘數對不起來（差 N 分鐘），檔案可能被改過」
paused_seconds = min(paused_minutes * 60, (e - s).num_seconds())
```

`apply`：名稱先 `trim()`，空字串改成 `UNNAMED_*`。依名稱找客戶，再在該客戶底下找同名專案（包含已封存的）；找不到就呼叫 `catalog::add_project`（新專案的顏色因此沿用「該客戶現有專案數 % 8」）。每筆紀錄用 `model::new_id()` 產生 id，`pause_started_at: None`，`extra` 空。

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test -p partimer-core csv_import::`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/partimer-core/src/csv_import.rs
git commit -m "feat(core): resolve CSV rows to entries and apply them to data"
```

---

### Task 5: Session 的轉出與轉入

**Files:**
- Create: `crates/partimer-core/src/session/moving.rs`
- Create: `crates/partimer-core/src/session/tests/moving.rs`
- Modify: `crates/partimer-core/src/session.rs`（檔尾加 `mod moving;` 與 `pub use moving::{CsvImportPlan, TransferInPlan};`）、`crates/partimer-core/src/lib.rs:17`（改成 `pub use session::{CsvImportPlan, Session, TransferInPlan};`，Tauri 層要在放開 lock 時拿著 plan）、`crates/partimer-core/src/session/tests.rs`（檔尾加 `mod moving;`）

**Interfaces:**
- Consumes: Task 1 的 `write_snapshot`、`SnapshotKind`、`Action::*`；Task 2 的 `parse_transfer`、`counts`、`is_empty`、`date_range`；`Session` 的私有方法 `running`、`replace_draft`、`selected_title`、`reset_reminders`、`persist`、`log`
- Produces:
  - `pub struct TransferInPlan { data: Data, pub confirm_text: String }`
  - `impl<F: Fs> Session<F>`：
    - `pub fn transfer_out(&self, now: DateTime<Utc>) -> Result<Vec<u8>, String>`：計時中（含暫停）回 `Err("請先按 STOP 結束計時再轉出")`，否則回 `to_json(&self.data)`
    - `pub fn note_transfer_out(&self, now: DateTime<Utc>)`：記一行 `TRANSFER_OUT`，note `entries={n}`
    - `pub fn prepare_transfer_in(&self, bytes: &[u8]) -> Result<TransferInPlan, String>`
    - `pub fn transfer_in(&mut self, plan: TransferInPlan, now: DateTime<Utc>) -> Result<String, String>`：回傳成功訊息
  - `pub(crate) const SAVE_BLOCKED_MOVE: &str = "資料檔目前不能寫入（由較新版建立或讀不到），為避免覆蓋，這次開啟期間不能轉入或匯入。";`

`confirm_text`（spec §4.2 的文字，數字和日期代入）：

```text
將轉入 {X} 個客戶、{Y} 個專案、{Z} 筆紀錄（{first} ～ {last}）。
```
沒有紀錄時省略括號那段。這台電腦 `!is_empty` 時，換行加上：
```text
這台電腦目前的 {N} 個客戶、{M} 個專案、{K} 筆紀錄會全部被取代。原本的資料會另存為 data.before-transfer-…json。
```
計時中再換行加 `計時中的那筆也會被取代。`

`transfer_in` 的步驟：`save_blocked` → `Err(SAVE_BLOCKED_MOVE)`；`!is_empty(&self.data)` 時 `write_snapshot(Transfer)`，失敗回 `Err(format!("無法另存目前的資料，沒有轉入（{e}）"))`；記下被取代的筆數；`self.data = plan.data`；`replace_draft(self.selected_title(), String::new())`；`reset_reminders(now)`；`persist(now)`；記 `TRANSFER_IN`，note `entries={Z} replaced={K} snapshot={檔名或 -}`。成功訊息是 `已轉入 {Z} 筆紀錄。`，有另存時接 `原本的資料另存為 {檔名}。`

- [ ] **Step 1: 寫失敗的測試**（`session/tests/moving.rs`，`use super::*;` 取用 `open`、`seed`、`base_data`、`on_disk`、`log_text`、`count_action`、`at`）

```rust
#[test] fn transfer_out_refuses_while_running()        // start 後 → Err("請先按 STOP 結束計時再轉出")
#[test] fn transfer_out_is_data_json_bytes()           // == to_json(&base_data())；note_transfer_out 後 count_action("TRANSFER_OUT") == 1
#[test] fn transfer_round_trip_replaces_everything()   // A 轉出 → B（有別的資料）轉入 → on_disk(B) == A 的資料（含 settings.widget x/y、extra、quarantined）
#[test] fn transfer_in_into_empty_skips_snapshot()     // 空的 session → 沒有 data.before-transfer-* 檔；訊息 "已轉入 N 筆紀錄。"
#[test] fn transfer_in_snapshots_current_memory()      // B 設 fail_renames 後改一筆（dirty，磁碟上是舊的；另存只用 write_sync 所以照樣能寫）→ 另存檔內容 == B 記憶體中的資料，不是磁碟上的
#[test] fn transfer_in_snapshot_failure_changes_nothing() // fail_write_syncs → Err，B 的記憶體與磁碟都不變
#[test] fn transfer_in_while_running_drops_entry_without_stop() // B 計時中且已觸發 12h 提醒 → 轉入後 timer_status 是 Idle、count_action("STOP") 不變、tick 不再回 LongRunning
#[test] fn transfer_in_resets_draft_and_conflict()      // B 有未存草稿與 EditedElsewhere 提示 → 草稿 = 新資料的 selected_title + 空內容，generation 增加，提示消失
#[test] fn transfer_in_refused_when_save_blocked()      // 用較新 version 的 data.json 開啟 → Err(SAVE_BLOCKED_MOVE)
#[test] fn confirm_text_mentions_replacement_and_running() // B 有 2 筆、計時中 → 文字含 "會全部被取代"、"計時中的那筆也會被取代。"
#[test] fn snapshot_can_be_transferred_back_in()        // 讀回 data.before-transfer-* 再 prepare/transfer_in → 回到轉入前的資料
#[test] fn transfer_in_logs_counts()                    // log 有一行 TRANSFER_IN，含 entries= 與 replaced=
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test -p partimer-core session::tests::moving`
Expected: 編譯失敗

- [ ] **Step 3: 實作 `session/moving.rs` 的轉出／轉入方法**（照上面 Interfaces 的步驟）

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test -p partimer-core`
Expected: 全部 PASS

- [ ] **Step 5: Commit**

```bash
git add crates/partimer-core/src/session.rs crates/partimer-core/src/session/
git commit -m "feat(core): transfer all data out and in through the session"
```

---

### Task 6: Session 的匯入 CSV

**Files:**
- Modify: `crates/partimer-core/src/session/moving.rs`
- Modify: `crates/partimer-core/src/session/tests/moving.rs`

**Interfaces:**
- Consumes: Task 4 的 `prepare`、`apply`、`new_catalog_counts`、`CsvImport`；Task 1 的 `write_snapshot(Import)`
- Produces:
  - `pub struct CsvImportPlan { import: CsvImport, pub confirm_text: String }`
  - `pub fn prepare_csv_import(&self, files: &[(String, Vec<u8>)]) -> Result<CsvImportPlan, String>`
  - `pub fn import_csv(&mut self, plan: CsvImportPlan, now: DateTime<Utc>) -> Result<String, String>`
  - `pub(crate) const CSV_NEEDS_EMPTY: &str = "匯入 CSV 只能用在還沒有任何紀錄的時候。要在兩台電腦間搬資料，請改用設定裡的『轉入資料』。";`

`confirm_text`：

```text
將匯入 {Z} 筆紀錄（{first} ～ {last}），新增 {X} 個客戶、{Y} 個專案；略過重複 {W} 筆。時間依這台電腦的時區解讀。
```
`W == 0` 時省略「；略過重複 0 筆」。

`prepare_csv_import` 與 `import_csv` **都要**檢查：`save_blocked` → `SAVE_BLOCKED_MOVE`；`entries` 或 `quarantined_entries` 不是空的 → `CSV_NEEDS_EMPTY`。`import_csv`：`!is_empty` 時 `write_snapshot(Import)`，失敗就中止；`apply`；`replace_draft(self.selected_title(), String::new())`，只在沒計時時做（實際上一定沒在計時，因為沒有紀錄）；`persist`；記 `IMPORT_CSV`，note `files={F} entries={Z} skipped={W}`。成功訊息是 `已匯入 {Z} 筆紀錄。`

- [ ] **Step 1: 寫失敗的測試**

```rust
#[test] fn csv_round_trip_restores_entries()         // 整分鐘的資料跨兩個月，export_csv 每月一檔 → 空 session 匯入
                                                      // → 再 export_csv 兩個月的 bytes 和原本完全相同
#[test] fn csv_round_trip_with_seconds_within_a_minute() // 開始 09:30:59、結束 11:00:00 → 匯入後每筆分鐘數和原本差 ≤ 1
#[test] fn import_csv_refused_when_entries_exist()   // Err(CSV_NEEDS_EMPTY)
#[test] fn import_csv_refused_with_quarantined_entry()
#[test] fn import_csv_rechecks_entries_at_apply()    // prepare 後 start/stop 一筆 → import_csv 回 Err(CSV_NEEDS_EMPTY)，資料不變
#[test] fn import_csv_allowed_with_only_catalog()    // 只有客戶專案 → 成功，留下 data.before-import-* 另存檔
#[test] fn import_csv_into_empty_skips_snapshot()
#[test] fn import_csv_logs_one_line()                // count_action("IMPORT_CSV") == 1，note 含 files=2 entries= skipped=
#[test] fn import_csv_confirm_text()                 // 含 "新增 1 個客戶、2 個專案"、"略過重複 3 筆"、"時間依這台電腦的時區解讀"
```

- [ ] **Step 2: 跑測試確認失敗**

Run: `cargo test -p partimer-core session::tests::moving`
Expected: 新測試編譯失敗

- [ ] **Step 3: 實作兩個方法**

- [ ] **Step 4: 跑測試確認通過**

Run: `cargo test -p partimer-core && cargo clippy -p partimer-core --all-targets -- -D warnings`
Expected: PASS，沒有 warning

- [ ] **Step 5: Commit**

```bash
git add crates/partimer-core/src/session/
git commit -m "feat(core): import exported CSV files into an empty session"
```

---

### Task 7: Tauri command 與小時鐘重新定位

**Files:**
- Create: `src-tauri/src/moving.rs`
- Modify: `src-tauri/src/commands.rs:48`（`session` 改成 `pub(crate)`）、`:290`（`export_error` 改成 `pub(crate)`）、`:58`（`settings_changed` 改成 `pub(crate)`）
- Modify: `src-tauri/src/windows.rs`（新增 `reposition_widget`）
- Modify: `src-tauri/src/lib.rs`（`mod moving;`，在 `generate_handler!` 註冊三個 command）

**Interfaces:**
- Consumes: Task 5、6 的 `Session` 方法
- Produces（前端 `invoke` 的名稱）：
  - `#[tauri::command] pub async fn transfer_out(app, state) -> Res<Option<String>>`
  - `#[tauri::command] pub async fn transfer_in(app, state) -> Res<Option<String>>`
  - `#[tauri::command] pub async fn import_csv(app, state) -> Res<Option<String>>`
  - 都是 `None` = 使用者取消，`Some(msg)` = 成功訊息
  - `pub fn reposition_widget(app: &AppHandle)`（`windows.rs`）

流程（**跳對話框時不能拿著 session lock**）：

| command | 步驟 |
|---|---|
| `transfer_out` | lock → `transfer_out(now)?`，記下 bytes → unlock → `save_file`（標題「轉出資料」，檔名 `partimer-transfer-{本機 YYYY-MM-DD}.json`，篩選器 `("轉移檔", ["json"])`）→ 取消回 `None` → `std::fs::write`，失敗用 `export_error` → lock → `note_transfer_out` → `Some("已轉出到 {檔名}")` |
| `transfer_in` | `pick_file`（標題「轉入資料」，只列 json）→ 讀檔失敗回 `無法讀取這個檔案（{e}）` → lock → `prepare_transfer_in` → unlock → `message(confirm_text)`，標題「轉入資料」，`MessageDialogKind::Warning`，`OkCancelCustom("轉入", "取消")` → 取消回 `None` → lock → `transfer_in` → unlock → `changed`、`settings_changed`、`reposition_widget` |
| `import_csv` | `pick_files`（標題「匯入 CSV」，`("CSV", ["csv"])`）→ 讀每個檔，`file` 用檔名（不含資料夾）→ lock → `prepare_csv_import` → unlock → 確認（標題「匯入 CSV」，`OkCancelCustom("匯入", "取消")`）→ lock → `import_csv` → unlock → `changed` |

`reposition_widget`：在 `on_worker` 裡先執行 `ensure_widget_now`；如果小時鐘視窗存在，用和 `build_widget` 一樣的 `widget_position(saved, &screens, primary)` 算出位置，經 `to_logical` 後呼叫 `set_position(LogicalPosition)`。不要 destroy 再重建：同一個 label 立刻重建可能會撞到還沒釋放的舊視窗。

- [ ] **Step 1: 寫 command 與 `reposition_widget`**（這一層只有對話框和呼叫，邏輯都在 core 測過了；`windows.rs` 現有的 `widget_position` 測試已經涵蓋位置在螢幕外的情況）

- [ ] **Step 2: 編譯與 lint**

Run: `rust-ai-lean diag check -p partimer && cargo clippy -p partimer --all-targets -- -D warnings && cargo test --workspace`
Expected: 沒有錯誤、沒有 warning，測試全過

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/
git commit -m "feat(app): transfer out/in and CSV import commands with native dialogs"
```

---

### Task 8: 前端按鈕

**Files:**
- Modify: `src/lib/api.ts:222`、`:243` 附近
- Modify: `src/settings/Settings.svelte:191-195`（「資料」區塊）
- Modify: `src/records/Records.svelte:109-119`、`:203`

**Interfaces:**
- Consumes: Task 7 的 command 名稱
- Produces（`api`）：
  - `transferOut: () => invoke<string | null>("transfer_out")`
  - `transferIn: () => invoke<string | null>("transfer_in")`
  - `importCsv: () => invoke<string | null>("import_csv")`

- [ ] **Step 1: 改畫面**
  - Settings「資料」區塊：在「打開資料夾」下方加 `<h3>換電腦</h3>`，以及「轉出資料」「轉入資料」兩個按鈕。執行中兩個都 `disabled`。成功訊息另外用一個 `info` 狀態顯示，不要套 `.error` 樣式；錯誤照樣放進現有的 `message`。轉入成功後，畫面靠 `settings-changed`／`state-changed` 自己更新（Settings 已經有監聽）。
  - Records：「匯出 CSV」旁加「匯入 CSV」，用和 `exportCsv` 一樣的 busy／message 寫法。成功訊息是回傳的字串，`null` 時不顯示任何東西。

- [ ] **Step 2: 檢查**

Run: `npm run check && npm test && npm run build`
Expected: svelte-check 0 錯 0 warning，測試全過，build 成功

- [ ] **Step 3: Commit**

```bash
git add src/
git commit -m "feat(ui): add transfer buttons to settings and CSV import to records"
```

---

### Task 9: 文件與實機確認

**Files:**
- Modify: `README.md`（「紀錄、設定與 CSV 匯出」、「資料放在哪裡」、「備份與還原」）
- Modify: `docs/superpowers/specs/2026-11-06-partimer-design.md` §9.4
- Modify: `docs/verification-checklist.md`（「Common: settings window」加 S12–S14，「Common: records window」加 R20，格式照現有項目，每項三個平台的勾選框）

- [ ] **Step 1: 改 README**
  - 「換電腦或自己備份」改寫：優先用「轉出資料／轉入資料」，**轉入會覆蓋這台電腦上的所有資料（包括計時中的那筆）**，轉入前會另存原本的資料。手上只剩 CSV 時，用紀錄視窗的「匯入 CSV」，只能在沒有紀錄時使用，會掉秒數、顏色、封存狀態、設定，時間依匯入電腦的時區解讀。「複製整個資料夾」保留為進階做法。
  - 資料夾表格加 `data.before-transfer-YYYYMMDD-HHMMSS.json`、`data.before-import-YYYYMMDD-HHMMSS.json`：轉入／匯入前的另存檔，要回到之前的狀態就用「轉入資料」選它，用不到可以刪。
  - 設定視窗那段加「換電腦：轉出資料、轉入資料」。
- [ ] **Step 2: 主設計 §9.4 動作清單加 `TRANSFER_OUT`、`TRANSFER_IN`、`IMPORT_CSV`**
- [ ] **Step 3: 驗證清單**
  - **S12** 跨平台轉移：A 平台「轉出資料」→ B 平台「轉入資料」，客戶、專案、顏色、封存、紀錄（含秒數）、設定全部一致。
  - **S13** 轉入覆蓋計時中的資料：確認框寫出會被取代的筆數與「計時中的那筆也會被取代。」；轉入後面板回到未計時，資料夾裡有 `data.before-transfer-…json`。
  - **S14** 轉入後，小時鐘出現在新螢幕上看得到的位置（舊座標超出螢幕時在右上角）。
  - **R20** 一次選多個月的 CSV 匯入到全新安裝：確認框筆數正確，面板與紀錄視窗立刻更新；已有紀錄時顯示改用「轉入資料」的提示。
- [ ] **Step 4: 實機跑一次**

用 `run` skill 啟動 app（`npm run tauri dev`）：轉出 → 改一點資料 → 轉入剛剛的檔 → 資料還原，而且有另存檔；匯出兩個月的 CSV → 把資料夾換成空的重開 → 匯入兩個檔 → 紀錄視窗與原本一致。附上截圖或觀察結果。

- [ ] **Step 5: 全部驗證**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && npm run check && npm test`
Expected: 全過

- [ ] **Step 6: Commit**

```bash
git add README.md docs/
git commit -m "docs: describe transferring data and importing CSV"
```

CHANGELOG 不在這裡改，發版時照 `docs/releasing.md` 加上該版的段落。
