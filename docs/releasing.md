# 發佈（維護者用）

## 發佈新版本

1. 把 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`package.json` 的版本改成新版本（例如 `0.1.1`），在 `CHANGELOG.md` 寫好這一版的內容，提交到 `main`。
2. 打 tag 並推上去：`git tag v0.1.1 && git push origin v0.1.1`。測試版用 `v0.1.1-rc.1`（版本號比對時會忽略 `-rc.N`）。
3. Release workflow 會先檢查 tag 與 `tauri.conf.json` 的版本相同、整個 workspace 測試通過，再建立草稿 Release，三個平台各自把安裝檔上傳到同一個草稿。
4. 到 GitHub 的 Releases 頁面打開草稿，確認五個安裝檔都在（檔名見 `docs/verification-checklist.md` 開頭），把 `CHANGELOG.md` 這一版的內容貼到說明，再按「Publish release」。

## 重新發佈（同一個版本重跑）

某個平台建置失敗、或發現要修正後用同一個版本號重來時：

1. **刪掉草稿 Release**：Releases 頁面打開該版本的草稿 →「Delete」（或 `gh release delete v0.1.1 --yes`）。若只是某個平台的 job 在上傳前就失敗（例如 runner 問題）、程式碼不用改，也可以不刪草稿，直接在 Actions 頁面按「Re-run failed jobs」，然後回到上面「發佈新版本」的第 4 步確認草稿。
2. **刪掉 tag**（本機與遠端都要）：`git tag -d v0.1.1 && git push origin :refs/tags/v0.1.1`。
3. 修正並提交到 `main` 後，**重新打 tag 並推上去**：`git tag v0.1.1 && git push origin v0.1.1`。Workflow 會從頭再跑一次（檢查、建立新草稿、三個平台建置）。

已經按過「Publish release」的版本不要重用版本號：使用者可能已經下載，請改發下一個版本（例如 `v0.1.2`）。
