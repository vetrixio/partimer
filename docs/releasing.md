# 發佈（維護者用）

## 發佈新版本

1. 把 `src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`、`crates/partimer-core/Cargo.toml`、`package.json` 的版本改成新版本（例如 `0.1.4`；`package.json` 用 `npm version 0.1.4 --no-git-tag-version`，`Cargo.lock` 跑一次 `cargo check` 就會更新），在 `CHANGELOG.md` 加上 `## v0.1.4 - YYYY-MM-DD` 這一段，提交並推到 `main`。
2. 打 tag 並推上去：`git tag -a v0.1.4 -m v0.1.4 && git push origin v0.1.4`。不用先等 `main` 的 CI，Release workflow 自己會跑測試。
3. Release workflow 會：
   - **verify**：檢查 tag 與 `tauri.conf.json` 的版本相同、`CHANGELOG.md` 有這一版的段落、整個 workspace 與前端測試通過。任何一項失敗就停止，不會建立 Release。
   - **create-release**：建立並**直接發佈** Release，標題是版本號（例如 `v0.1.4`），說明是 `CHANGELOG.md` 這一版的內容（`scripts/changelog-section.sh` 取出）。
   - **build**：三個平台同時打包，**哪個先打完就先上傳**，所以 Release 頁上的檔案會陸續出現（Windows 最快、macOS 最慢）。
4. 全部完成後，確認 Release 頁有 6 個檔案：macOS 的 `.dmg` 與 `.app.tar.gz`、Windows 的 `-setup.exe` 與 `_portable.exe`、Ubuntu 的 `.deb` 與 `.AppImage`。

## 建置快取

`main` 每次推送時，CI 的 `release-cache` job 會在三個平台各編譯一次正式版（不上傳），把編譯好的相依套件存成快取。打 tag 時 Release workflow 只能讀 `main` 存的快取（GitHub 的限制，讀不到其他 tag 的），所以發版時只需要重新編譯 partimer 自己的程式。

- 推到 `main` 後馬上打 tag 也可以：`release-cache` 還沒跑完時，會改用前一次 `main` 存的快取。
- 快取失效（例如升級 Rust 或大量更新相依套件）時，第一次發版會比較久，之後就恢復。

## 某個平台建置失敗

Release 已經發佈，只是缺那個平台的檔案：

- **不用改程式**（例如 runner 問題）：到 Actions 頁面打開該次 Release workflow，按「Re-run failed jobs」。重跑時會沿用同一個 Release，補上缺的檔案。
- **要改程式**：不要重用版本號（使用者可能已經下載其他平台的檔案）。修正後照「發佈新版本」發下一個版本（例如 `v0.1.5`），並在 `CHANGELOG.md` 註明上一版缺哪個平台。

## verify 失敗

還沒建立 Release，可以修好後用同一個版本號重來：

1. 刪掉 tag（本機與遠端）：`git tag -d v0.1.4 && git push origin :refs/tags/v0.1.4`。
2. 修正並推到 `main`，再重新打 tag 並推上去。
