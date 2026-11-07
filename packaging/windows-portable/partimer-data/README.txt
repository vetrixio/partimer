這個資料夾讓打工人計時器以「免安裝模式」執行。

所有資料（data.json、每日備份、活動紀錄）都存在這個資料夾裡，
可以把整個 partimer 資料夾複製到隨身碟或另一台電腦，資料會跟著走。

- 請讓這個 partimer-data 資料夾和 partimer.exe 放在同一層。
- 刪掉或改名這個資料夾，partimer 會改用 %APPDATA%\partimer（和安裝版相同的位置）。
- 不要放在 C:\Program Files 這類需要管理員權限才能寫入的地方，否則無法存檔。
- 「開機自動啟動」記住的是 partimer.exe 目前的位置；搬移資料夾後請到設定重新打開一次。
- 執行需要 Microsoft Edge WebView2 Runtime（Windows 11 與大部分 Windows 10 已內建）。
