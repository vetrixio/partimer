//! Parse CSV files written by `report::export_csv` back into rows (exact inverse of its format).

use std::collections::{HashMap, HashSet};
use std::fmt;

use chrono::{DateTime, Duration, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Utc};

use crate::catalog::{self, COLOR_COUNT};
use crate::model::{Data, Entry, Id, new_id};
use crate::report::CSV_HEADER;
use crate::zone::LocalZone;

pub(crate) const UNNAMED_CLIENT: &str = "未命名客戶";
pub(crate) const UNNAMED_PROJECT: &str = "未命名專案";

/// Largest gap (minutes) tolerated between the end time and the minutes column; the export
/// rounds start, end and minutes independently.
const TOLERANCE_MINUTES: i64 = 2;
const MAX_LISTED_ERRORS: usize = 5;

/// One data row of an exported CSV. `line` is the physical line where the record starts
/// (the header is line 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvRow {
    pub file: String,
    pub line: usize,
    pub date: NaiveDate,
    pub client: String,
    pub project: String,
    pub start: NaiveTime,
    pub end: NaiveTime,
    pub minutes: i64,
    pub hours: String,
    pub paused_minutes: i64,
    pub title: String,
    pub content: String,
}

/// Everything except `file` and `line`; two rows with the same key are the same record.
pub type RowKey<'a> = (
    NaiveDate,
    &'a str,
    &'a str,
    NaiveTime,
    NaiveTime,
    i64,
    &'a str,
    i64,
    &'a str,
    &'a str,
);

impl CsvRow {
    pub fn key(&self) -> RowKey<'_> {
        (
            self.date,
            &self.client,
            &self.project,
            self.start,
            self.end,
            self.minutes,
            &self.hours,
            self.paused_minutes,
            &self.title,
            &self.content,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowError {
    pub file: String,
    pub line: Option<usize>,
    pub problem: String,
}

impl fmt::Display for RowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "「{}」第 {line} 行：{}", self.file, self.problem),
            None => write!(f, "「{}」：{}", self.file, self.problem),
        }
    }
}

type Cell = (String, bool);

/// RFC 4180 records as `(start line, cells)`; each cell remembers whether it was quoted.
/// CR, LF and CRLF all end a record outside quotes. The second value is the start line of
/// a quote that was never closed.
fn split_records(s: &str) -> (Vec<(usize, Vec<Cell>)>, Option<usize>) {
    let mut records = Vec::new();
    let mut cells: Vec<Cell> = Vec::new();
    let mut cell = String::new();
    let mut quoted = false;
    let mut in_quotes = false;
    let mut line = 1;
    let mut start_line = 1;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    cell.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                cell.push(c);
                if c == '\r' && chars.peek() == Some(&'\n') {
                    cell.push('\n');
                    chars.next();
                }
                if c == '\n' || c == '\r' {
                    line += 1;
                }
            }
            continue;
        }
        match c {
            '"' if cell.is_empty() && !quoted => {
                in_quotes = true;
                quoted = true;
            }
            ',' => cells.push((std::mem::take(&mut cell), std::mem::take(&mut quoted))),
            '\r' | '\n' => {
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                cells.push((std::mem::take(&mut cell), std::mem::take(&mut quoted)));
                records.push((start_line, std::mem::take(&mut cells)));
                line += 1;
                start_line = line;
            }
            _ => cell.push(c),
        }
    }
    if in_quotes {
        return (records, Some(start_line));
    }
    if !cell.is_empty() || quoted || !cells.is_empty() {
        cells.push((cell, quoted));
        records.push((start_line, cells));
    }
    (records, None)
}

/// Inverse of `report::csv_field`'s injection guard: a quoted cell that starts with TAB
/// followed by `= + - @ TAB CR` had that TAB added by the exporter.
fn unneutralize(cell: Cell) -> String {
    let (s, quoted) = cell;
    let mut it = s.chars();
    if quoted
        && it.next() == Some('\t')
        && matches!(it.next(), Some('=' | '+' | '-' | '@' | '\t' | '\r'))
    {
        s[1..].to_string()
    } else {
        s
    }
}

fn parse_date(v: &str) -> Option<NaiveDate> {
    if v.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(v, "%Y-%m-%d").ok()
}

fn parse_time(v: &str) -> Option<NaiveTime> {
    if v.len() != 5 || v.as_bytes()[2] != b':' {
        return None;
    }
    NaiveTime::parse_from_str(v, "%H:%M").ok()
}

fn parse_count(v: &str) -> Option<i64> {
    v.parse::<i64>().ok().filter(|n| *n >= 0)
}

fn parse_record(file: &str, line: usize, cells: Vec<Cell>) -> Result<CsvRow, String> {
    if cells.len() != 10 {
        return Err(format!("欄位數是 {}，應為 10", cells.len()));
    }
    let mut v: Vec<String> = cells.into_iter().map(unneutralize).collect();
    let content = v.pop().unwrap_or_default();
    let title = v.pop().unwrap_or_default();
    let paused = v.pop().unwrap_or_default();
    let hours = v.pop().unwrap_or_default();
    let minutes = v.pop().unwrap_or_default();
    let end = v.pop().unwrap_or_default();
    let start = v.pop().unwrap_or_default();
    let project = v.pop().unwrap_or_default();
    let client = v.pop().unwrap_or_default();
    let date = v.pop().unwrap_or_default();
    Ok(CsvRow {
        file: file.to_string(),
        line,
        date: parse_date(&date)
            .ok_or_else(|| format!("日期『{date}』格式不對，應為 YYYY-MM-DD"))?,
        client,
        project,
        start: parse_time(&start)
            .ok_or_else(|| format!("開始時間『{start}』格式不對，應為 HH:MM"))?,
        end: parse_time(&end).ok_or_else(|| format!("結束時間『{end}』格式不對，應為 HH:MM"))?,
        minutes: parse_count(&minutes)
            .ok_or_else(|| format!("分鐘『{minutes}』應為 0 以上的整數"))?,
        hours,
        paused_minutes: parse_count(&paused)
            .ok_or_else(|| format!("暫停分鐘『{paused}』應為 0 以上的整數"))?,
        title,
        content,
    })
}

/// Parse one exported CSV. Every row is checked; all problems are returned together.
pub fn parse_file(file: &str, bytes: &[u8]) -> Result<Vec<CsvRow>, Vec<RowError>> {
    let whole = |line: Option<usize>, problem: &str| {
        vec![RowError {
            file: file.to_string(),
            line,
            problem: problem.to_string(),
        }]
    };
    let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes);
    let Ok(text) = std::str::from_utf8(bytes) else {
        return Err(whole(None, "檔案不是 UTF-8 編碼，可能被 Excel 另存過"));
    };
    let (records, unterminated) = split_records(text);
    let header: Vec<&str> = CSV_HEADER.split(',').collect();
    let header_ok = records.first().is_some_and(|(_, cells)| {
        cells
            .iter()
            .map(|c| c.0.as_str())
            .eq(header.iter().copied())
    });
    if !header_ok {
        return Err(whole(None, "第一行不是打工人計時器匯出的欄位名稱"));
    }

    let mut rows = Vec::new();
    let mut errors = Vec::new();
    for (line, cells) in records.into_iter().skip(1) {
        if cells.len() == 1 && cells[0].0.is_empty() && !cells[0].1 {
            continue;
        }
        match parse_record(file, line, cells) {
            Ok(row) => rows.push(row),
            Err(problem) => errors.push(RowError {
                file: file.to_string(),
                line: Some(line),
                problem,
            }),
        }
    }
    if let Some(line) = unterminated {
        errors.push(RowError {
            file: file.to_string(),
            line: Some(line),
            problem: "引號沒有成對".to_string(),
        });
    }
    if errors.is_empty() {
        Ok(rows)
    } else {
        Err(errors)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportRow {
    pub client: String,
    pub project: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub paused_seconds: i64,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvImport {
    pub rows: Vec<ImportRow>,
    pub files: usize,
    pub skipped_duplicates: usize,
    pub first_date: NaiveDate,
    pub last_date: NaiveDate,
}

/// Turns the local date and HH:MM columns back into UTC instants. The CSV has no end date, so
/// the end is searched over the following days for the one whose length best matches
/// minutes + paused minutes.
fn resolve(row: &CsvRow, zone: &dyn LocalZone) -> Result<ImportRow, String> {
    let starts: Vec<DateTime<Utc>> = match zone.from_local(row.date.and_time(row.start)) {
        LocalResult::Single(t) => vec![t],
        LocalResult::Ambiguous(a, b) => vec![a, b],
        LocalResult::None => {
            return Err(format!(
                "開始時間 {} {} 在這台電腦的時區不存在（夏令時間）",
                row.date.format("%Y-%m-%d"),
                row.start.format("%H:%M")
            ));
        }
    };
    let Some(span) = row.minutes.checked_add(row.paused_minutes) else {
        return Err("分鐘數太大，檔案可能被改過".to_string());
    };
    let mut best: Option<(i64, DateTime<Utc>, DateTime<Utc>)> = None;
    // The true end is always within a day of the day that start + span lands on.
    let start_minutes = i64::from(row.start.hour() * 60 + row.start.minute());
    let Some(centre) = span.checked_add(start_minutes).map(|m| m / 1440) else {
        return Err("分鐘數太大，檔案可能被改過".to_string());
    };
    for k in centre.saturating_sub(1)..=centre.saturating_add(1) {
        let Some(day) = Duration::try_days(k).and_then(|n| row.date.checked_add_signed(n)) else {
            continue;
        };
        let end_local: NaiveDateTime = day.and_time(row.end);
        let ends = match zone.from_local(end_local) {
            LocalResult::Single(t) => vec![t],
            LocalResult::Ambiguous(a, b) => vec![a, b],
            LocalResult::None => continue,
        };
        for &s in &starts {
            for &e in &ends {
                if e < s {
                    continue;
                }
                let diff = ((e - s).num_minutes() - span).abs();
                if best.is_none_or(|(d, _, _)| diff < d) {
                    best = Some((diff, s, e));
                }
            }
        }
    }
    match best {
        Some((diff, start, end)) if diff <= TOLERANCE_MINUTES => Ok(ImportRow {
            client: row.client.clone(),
            project: row.project.clone(),
            start,
            end,
            paused_seconds: row
                .paused_minutes
                .saturating_mul(60)
                .min((end - start).num_seconds()),
            title: row.title.clone(),
            content: row.content.clone(),
        }),
        Some((diff, _, _)) => Err(format!(
            "結束時間和分鐘數對不起來（差 {diff} 分鐘），檔案可能被改過"
        )),
        None => Err("結束時間和分鐘數對不起來，檔案可能被改過".to_string()),
    }
}

pub fn prepare(files: &[(String, Vec<u8>)], zone: &dyn LocalZone) -> Result<CsvImport, String> {
    let mut errors: Vec<RowError> = Vec::new();
    let mut parsed: Vec<CsvRow> = Vec::new();
    for (name, bytes) in files {
        match parse_file(name, bytes) {
            Ok(rows) => parsed.extend(rows),
            Err(e) => errors.extend(e),
        }
    }
    let mut seen = HashSet::new();
    let mut skipped_duplicates = 0;
    let mut unique: Vec<&CsvRow> = Vec::new();
    for row in &parsed {
        if seen.insert(row.key()) {
            unique.push(row);
        } else {
            skipped_duplicates += 1;
        }
    }
    let mut rows = Vec::with_capacity(unique.len());
    for row in &unique {
        match resolve(row, zone) {
            Ok(r) => rows.push(r),
            Err(problem) => errors.push(RowError {
                file: row.file.clone(),
                line: Some(row.line),
                problem,
            }),
        }
    }
    if !errors.is_empty() {
        let mut lines: Vec<String> = errors
            .iter()
            .take(MAX_LISTED_ERRORS)
            .map(ToString::to_string)
            .collect();
        if errors.len() > MAX_LISTED_ERRORS {
            lines.push(format!("…還有 {} 筆", errors.len() - MAX_LISTED_ERRORS));
        }
        return Err(lines.join("\n"));
    }
    let (Some(first_date), Some(last_date)) = (
        unique.iter().map(|r| r.date).min(),
        unique.iter().map(|r| r.date).max(),
    ) else {
        return Err("選的檔案裡沒有任何紀錄".to_string());
    };
    rows.sort_by_key(|r| r.start);
    Ok(CsvImport {
        rows,
        files: files.len(),
        skipped_duplicates,
        first_date,
        last_date,
    })
}

fn names_of(row: &ImportRow) -> (String, String) {
    let pick = |s: &str, fallback: &str| {
        let t = s.trim();
        if t.is_empty() { fallback } else { t }.to_string()
    };
    (
        pick(&row.client, UNNAMED_CLIENT),
        pick(&row.project, UNNAMED_PROJECT),
    )
}

fn find_project(data: &Data, client: &str, project: &str) -> Option<Id> {
    let client = data.clients.iter().find(|c| c.name == client)?;
    data.projects
        .iter()
        .find(|p| p.client_id == client.id && p.name == project)
        .map(|p| p.id.clone())
}

/// How many clients and projects `apply` would create.
pub fn new_catalog_counts(data: &Data, imp: &CsvImport) -> (usize, usize) {
    let pairs: HashSet<(String, String)> = imp.rows.iter().map(names_of).collect();
    let clients: HashSet<&str> = pairs.iter().map(|(c, _)| c.as_str()).collect();
    let new_clients = clients
        .iter()
        .filter(|c| !data.clients.iter().any(|e| e.name == **c))
        .count();
    let new_projects = pairs
        .iter()
        .filter(|(c, p)| find_project(data, c, p).is_none())
        .count();
    (new_clients, new_projects)
}

/// Adds the rows as finished entries, creating missing clients and projects.
/// Returns how many entries were added.
pub fn apply(data: &mut Data, imp: CsvImport) -> usize {
    let mut ids: HashMap<(String, String), Id> = HashMap::new();
    let mut added = 0;
    let mut created = 0usize;
    for row in imp.rows {
        let names = names_of(&row);
        let project_id = match ids.get(&names) {
            Some(id) => id.clone(),
            None => {
                let found = find_project(data, &names.0, &names.1);
                let id = match found {
                    Some(id) => id,
                    None => {
                        // Names come out of `names_of` already normalised and the project was
                        // just looked up as missing, so `add_project` cannot refuse them.
                        let id = catalog::add_project(data, &names.0, &names.1)
                            .expect("names are normalised before add_project");
                        // Spec 5.4: new projects take colours 0..7 in creation order.
                        if let Some(p) = data.projects.iter_mut().find(|p| p.id == id) {
                            p.color = (created % COLOR_COUNT as usize) as u8;
                        }
                        created += 1;
                        id
                    }
                };
                ids.insert(names, id.clone());
                id
            }
        };
        data.entries.push(Entry {
            extra: Default::default(),
            id: new_id(),
            project_id,
            start: row.start,
            end: Some(row.end),
            title: row.title,
            content: row.content,
            paused_seconds: row.paused_seconds,
            pause_started_at: None,
        });
        added += 1;
    }
    added
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Client, Data, Entry, Project};
    use crate::report::{self, Filter};
    use crate::testutil::{tz, utc};

    fn entry(id: &str, start: &str, end: &str, title: &str, content: &str) -> Entry {
        Entry {
            extra: Default::default(),
            id: id.into(),
            project_id: "p1".into(),
            start: utc(start),
            end: Some(utc(end)),
            title: title.into(),
            content: content.into(),
            paused_seconds: 600,
            pause_started_at: None,
        }
    }

    fn export(client: &str, entries: Vec<Entry>) -> Vec<u8> {
        let mut d = Data::default();
        d.clients.push(Client {
            extra: Default::default(),
            id: "c1".into(),
            name: client.into(),
            archived: false,
        });
        d.projects.push(Project {
            extra: Default::default(),
            id: "p1".into(),
            client_id: "c1".into(),
            name: "官網改版".into(),
            color: 0,
            archived: false,
        });
        d.entries = entries;
        let f = Filter {
            year: 2026,
            month: 10,
            client_id: None,
            project_id: None,
        };
        report::export_csv(&d, &f, tz("Asia/Taipei").as_ref())
    }

    fn two_rows() -> Vec<u8> {
        export(
            "王小明",
            vec![
                entry(
                    "a",
                    "2026-10-05T01:00:00Z",
                    "2026-10-05T03:30:00Z",
                    "首頁",
                    "切版",
                ),
                entry("b", "2026-10-06T01:00:00Z", "2026-10-06T02:00:00Z", "", ""),
            ],
        )
    }

    fn err_text(r: Result<Vec<CsvRow>, Vec<RowError>>) -> Vec<String> {
        r.unwrap_err().iter().map(|e| e.to_string()).collect()
    }

    const HEAD: &str = "日期,客戶,專案,開始,結束,分鐘,小時,暫停分鐘,標題,內容";

    fn text(body: &str) -> Vec<u8> {
        format!("{HEAD}\r\n{body}").into_bytes()
    }

    #[test]
    fn parses_exported_csv() {
        let rows = parse_file("a.csv", &two_rows()).unwrap();
        assert_eq!(rows.len(), 2);
        let r = &rows[0];
        assert_eq!(r.file, "a.csv");
        assert_eq!(r.line, 2);
        assert_eq!(r.date, NaiveDate::from_ymd_opt(2026, 10, 5).unwrap());
        assert_eq!(r.client, "王小明");
        assert_eq!(r.project, "官網改版");
        assert_eq!(r.start, NaiveTime::from_hms_opt(9, 0, 0).unwrap());
        assert_eq!(r.end, NaiveTime::from_hms_opt(11, 30, 0).unwrap());
        assert_eq!(r.minutes, 140);
        assert_eq!(r.hours, "2.33");
        assert_eq!(r.paused_minutes, 10);
        assert_eq!(r.title, "首頁");
        assert_eq!(r.content, "切版");
        assert_eq!(rows[1].line, 3);
        assert_eq!(rows[1].title, "");
    }

    #[test]
    fn accepts_no_bom_and_lf() {
        let with = parse_file("a.csv", &two_rows()).unwrap();
        let s = String::from_utf8(two_rows()[3..].to_vec())
            .unwrap()
            .replace("\r\n", "\n");
        let without = parse_file("a.csv", s.as_bytes()).unwrap();
        assert_eq!(with, without);
    }

    #[test]
    fn unquotes_commas_quotes_and_newlines() {
        let bytes = export(
            "王小明",
            vec![
                entry(
                    "a",
                    "2026-10-05T01:00:00Z",
                    "2026-10-05T02:00:00Z",
                    "a,\"b\"",
                    "x\ny",
                ),
                entry(
                    "b",
                    "2026-10-06T01:00:00Z",
                    "2026-10-06T02:00:00Z",
                    "t",
                    "c",
                ),
            ],
        );
        let rows = parse_file("a.csv", &bytes).unwrap();
        assert_eq!(rows[0].title, "a,\"b\"");
        assert_eq!(rows[0].content, "x\ny");
        assert_eq!(rows[0].line, 2);
        // The first record spans two physical lines.
        assert_eq!(rows[1].line, 4);
    }

    #[test]
    fn strips_injection_tab() {
        let bytes = export(
            "=王",
            vec![entry(
                "a",
                "2026-10-05T01:00:00Z",
                "2026-10-05T02:00:00Z",
                "- 改 banner",
                "",
            )],
        );
        assert!(String::from_utf8_lossy(&bytes).contains("\"\t- 改 banner\""));
        let rows = parse_file("a.csv", &bytes).unwrap();
        assert_eq!(rows[0].title, "- 改 banner");
        assert_eq!(rows[0].client, "=王");
    }

    #[test]
    fn keeps_tab_not_added_by_export() {
        let bytes = text("2026-10-05,A,P,09:00,10:00,60,1.00,0,\"\tabc\",\r\n");
        let rows = parse_file("a.csv", &bytes).unwrap();
        assert_eq!(rows[0].title, "\tabc");
    }

    #[test]
    fn skips_blank_lines() {
        let bytes = text("\r\n2026-10-05,A,P,09:00,10:00,60,1.00,0,t,c\r\n\r\n");
        let rows = parse_file("a.csv", &bytes).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].line, 3);
    }

    #[test]
    fn rejects_wrong_header() {
        let errs = parse_file("a.csv", b"date,client\r\n").unwrap_err();
        assert_eq!(
            errs,
            vec![RowError {
                file: "a.csv".into(),
                line: None,
                problem: "第一行不是打工人計時器匯出的欄位名稱".into(),
            }]
        );
        assert_eq!(
            errs[0].to_string(),
            "「a.csv」：第一行不是打工人計時器匯出的欄位名稱"
        );
    }

    #[test]
    fn rejects_non_utf8() {
        // "日期" in Big5.
        let errs = err_text(parse_file("a.csv", &[0xA4, 0xE9, 0xB4, 0xC1, 0x0D, 0x0A]));
        assert_eq!(
            errs,
            vec!["「a.csv」：檔案不是 UTF-8 編碼，可能被 Excel 另存過"]
        );
    }

    #[test]
    fn rejects_slash_date_with_file_and_line() {
        let bytes = text(
            "2026-10-05,A,P,09:00,10:00,60,1.00,0,t,c\r\n2026/10/6,A,P,09:00,10:00,60,1.00,0,t,c\r\n",
        );
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["「a.csv」第 3 行：日期『2026/10/6』格式不對，應為 YYYY-MM-DD"]
        );
    }

    #[test]
    fn rejects_single_digit_time() {
        let bytes = text("2026-10-05,A,P,9:3,10:00,60,1.00,0,t,c\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["「a.csv」第 2 行：開始時間『9:3』格式不對，應為 HH:MM"]
        );
        let bytes = text("2026-10-05,A,P,09:00,10:7,60,1.00,0,t,c\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["「a.csv」第 2 行：結束時間『10:7』格式不對，應為 HH:MM"]
        );
    }

    #[test]
    fn rejects_wrong_column_count() {
        let bytes = text("2026-10-05,A,P,09:00\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["「a.csv」第 2 行：欄位數是 4，應為 10"]
        );
    }

    #[test]
    fn rejects_negative_minutes() {
        let bytes = text(
            "2026-10-05,A,P,09:00,10:00,-5,1.00,0,t,c\r\n2026-10-05,A,P,09:00,10:00,60,1.00,x,t,c\r\n",
        );
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec![
                "「a.csv」第 2 行：分鐘『-5』應為 0 以上的整數",
                "「a.csv」第 3 行：暫停分鐘『x』應為 0 以上的整數",
            ]
        );
    }

    #[test]
    fn rejects_unterminated_quote() {
        let bytes = text("2026-10-05,A,P,09:00,10:00,60,1.00,0,\"oops,c\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["「a.csv」第 2 行：引號沒有成對"]
        );
    }

    #[test]
    fn collects_all_row_errors() {
        let bytes = text(
            "bad,A,P,09:00,10:00,60,1.00,0,t,c\r\n2026-10-05,A\r\n2026-10-05,A,P,09:00,10:00,x,1.00,0,t,c\r\n2026-10-05,A,P,09:00,10:00,60,1.00,0,t,c\r\n",
        );
        assert_eq!(parse_file("a.csv", &bytes).unwrap_err().len(), 3);
    }

    #[test]
    fn keeps_crlf_inside_quotes() {
        let bytes = text("2026-10-05,A,P,09:00,10:00,60,1.00,0,t,\"x\r\ny\"\r\n");
        assert_eq!(parse_file("a.csv", &bytes).unwrap()[0].content, "x\r\ny");
    }

    #[test]
    fn key_ignores_file_and_line() {
        let a = parse_file("a.csv", &two_rows()).unwrap();
        let b = parse_file("b.csv", &two_rows()).unwrap();
        assert_eq!(a[0].key(), b[0].key());
        assert_ne!(a[0].key(), a[1].key());
    }

    fn files(rows: &[&str]) -> Vec<(String, Vec<u8>)> {
        vec![("a.csv".to_string(), text(&rows.join("\r\n")))]
    }

    fn prep(zone: &str, rows: &[&str]) -> Result<CsvImport, String> {
        prepare(&files(rows), tz(zone).as_ref())
    }

    fn one(zone: &str, row: &str) -> ImportRow {
        prep(zone, &[row]).unwrap().rows.remove(0)
    }

    #[test]
    fn resolves_same_day_entry() {
        let r = one(
            "Asia/Taipei",
            "2026-10-06,王小明,官網改版,09:30,11:00,90,1.50,0,,",
        );
        assert_eq!(r.start, utc("2026-10-06T01:30:00Z"));
        assert_eq!(r.end, utc("2026-10-06T03:00:00Z"));
        assert_eq!(r.paused_seconds, 0);
    }

    #[test]
    fn resolves_overnight_entry() {
        let r = one("Asia/Taipei", "2026-10-06,A,B,23:00,01:30,150,2.50,0,,");
        assert_eq!(r.start, utc("2026-10-06T15:00:00Z"));
        assert_eq!(r.end, utc("2026-10-06T17:30:00Z"));
    }

    #[test]
    fn resolves_entry_longer_than_a_day() {
        let r = one("Asia/Taipei", "2026-10-06,A,B,09:00,10:00,1500,25.00,0,,");
        assert_eq!(r.end, utc("2026-10-07T02:00:00Z"));
    }

    #[test]
    fn uses_pause_to_pick_the_day() {
        let r = one("Asia/Taipei", "2026-10-06,A,B,09:00,09:30,30,0.50,1440,,");
        assert_eq!(r.end, utc("2026-10-07T01:30:00Z"));
        assert_eq!(r.paused_seconds, 1440 * 60);
    }

    #[test]
    fn resolves_dst_repeated_hour() {
        let r = one("America/New_York", "2026-11-01,A,B,01:30,01:50,80,1.33,0,,");
        assert_eq!(r.start, utc("2026-11-01T05:30:00Z"));
        assert_eq!(r.end, utc("2026-11-01T06:50:00Z"));
    }

    #[test]
    fn rejects_dst_gap_start() {
        let e = prep(
            "America/New_York",
            &["2026-03-08,A,B,02:30,03:30,60,1.00,0,,"],
        )
        .unwrap_err();
        assert!(
            e.contains("開始時間 2026-03-08 02:30 在這台電腦的時區不存在（夏令時間）"),
            "{e}"
        );
        assert!(e.contains("「a.csv」第 2 行"), "{e}");
    }

    #[test]
    fn dst_spring_forward_long_overnight_entry_resolves() {
        let r = one(
            "America/New_York",
            "2026-03-07,A,B,23:30,00:10,1420,23.67,0,,",
        );
        assert_eq!(r.end, utc("2026-03-09T04:10:00Z"));
    }

    #[test]
    fn mismatch_without_any_candidate_has_no_minute_count() {
        let e = prep(
            "Asia/Taipei",
            &["2026-10-06,A,B,09:00,10:00,999999999999999,0,0,,"],
        )
        .unwrap_err();
        assert!(
            e.contains("結束時間和分鐘數對不起來，檔案可能被改過"),
            "{e}"
        );
        assert!(!e.contains("差"), "{e}");
    }

    #[test]
    fn tolerates_two_minute_rounding() {
        let r = one("Asia/Taipei", "2026-10-06,A,B,09:00,10:00,58,0.97,0,,");
        assert_eq!(r.end, utc("2026-10-06T02:00:00Z"));
    }

    #[test]
    fn rejects_mismatch_over_two_minutes() {
        let e = prep("Asia/Taipei", &["2026-10-06,A,B,09:00,10:00,57,0.95,0,,"]).unwrap_err();
        assert!(
            e.contains("結束時間和分鐘數對不起來（差 3 分鐘），檔案可能被改過"),
            "{e}"
        );
    }

    #[test]
    fn clamps_pause_to_duration() {
        let r = one("Asia/Taipei", "2026-10-06,A,B,09:00,09:00,0,0.00,1,,");
        assert_eq!(r.paused_seconds, 0);
    }

    #[test]
    fn duplicate_rows_across_files_are_skipped() {
        let bytes = two_rows();
        let f = vec![
            ("a.csv".to_string(), bytes.clone()),
            ("b.csv".to_string(), bytes),
        ];
        let imp = prepare(&f, tz("Asia/Taipei").as_ref()).unwrap();
        assert_eq!(imp.rows.len(), 2);
        assert_eq!(imp.skipped_duplicates, 2);
        assert_eq!(imp.files, 2);
        assert_eq!(
            imp.first_date,
            NaiveDate::from_ymd_opt(2026, 10, 5).unwrap()
        );
        assert_eq!(imp.last_date, NaiveDate::from_ymd_opt(2026, 10, 6).unwrap());
        assert!(imp.rows[0].start < imp.rows[1].start);
    }

    #[test]
    fn error_list_caps_at_five() {
        let rows: Vec<String> = (0..7)
            .map(|i| format!("2026-10-06,A,B,09:00,10:00,5,0.08,0,t{i},"))
            .collect();
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        let e = prep("Asia/Taipei", &refs).unwrap_err();
        let lines: Vec<&str> = e.split('\n').collect();
        assert_eq!(lines.len(), 6, "{e}");
        assert_eq!(lines[5], "…還有 2 筆");
    }

    #[test]
    fn empty_selection_is_an_error() {
        assert_eq!(
            prep("Asia/Taipei", &[]).unwrap_err(),
            "選的檔案裡沒有任何紀錄"
        );
    }

    fn import_of(rows: &[(&str, &str)]) -> CsvImport {
        let rows: Vec<String> = rows
            .iter()
            .enumerate()
            .map(|(i, (c, p))| format!("2026-10-06,{c},{p},09:00,10:00,60,1.00,0,t{i},"))
            .collect();
        let refs: Vec<&str> = rows.iter().map(String::as_str).collect();
        prep("Asia/Taipei", &refs).unwrap()
    }

    fn data_with_archived() -> Data {
        let mut d = Data::default();
        d.clients.push(Client {
            extra: Default::default(),
            id: "c1".into(),
            name: "王小明".into(),
            archived: true,
        });
        d.projects.push(Project {
            extra: Default::default(),
            id: "p1".into(),
            client_id: "c1".into(),
            name: "官網改版".into(),
            color: 3,
            archived: true,
        });
        d
    }

    #[test]
    fn apply_reuses_existing_names_including_archived() {
        let mut d = data_with_archived();
        let imp = import_of(&[("王小明", "官網改版")]);
        assert_eq!(new_catalog_counts(&d, &imp), (0, 0));
        assert_eq!(apply(&mut d, imp), 1);
        assert_eq!(d.clients.len(), 1);
        assert_eq!(d.projects.len(), 1);
        assert_eq!(d.entries.len(), 1);
        assert_eq!(d.entries[0].project_id, "p1");
        assert_eq!(d.entries[0].pause_started_at, None);
        assert!(d.entries[0].end.is_some());
    }

    #[test]
    fn apply_trims_names_and_fills_unnamed() {
        let mut d = data_with_archived();
        let imp = import_of(&[(" 王小明 ", " 官網改版 "), ("  ", "  ")]);
        assert_eq!(apply(&mut d, imp), 2);
        assert_eq!(d.entries[0].project_id, "p1");
        let c = d.clients.iter().find(|c| c.name == UNNAMED_CLIENT).unwrap();
        let p = d
            .projects
            .iter()
            .find(|p| p.name == UNNAMED_PROJECT)
            .unwrap();
        assert_eq!(p.client_id, c.id);
        assert_eq!(d.entries[1].project_id, p.id);
    }

    #[test]
    fn apply_colors_rotate_in_creation_order() {
        let mut d = Data::default();
        let imp = import_of(&[("甲", "一"), ("乙", "二")]);
        apply(&mut d, imp);
        let colors: Vec<u8> = d.projects.iter().map(|p| p.color).collect();
        assert_eq!(colors, vec![0, 1]);
    }

    #[test]
    fn new_catalog_counts_matches_apply() {
        let mut d = data_with_archived();
        let imp = import_of(&[
            ("王小明", "新專案"),
            ("乙", "x"),
            ("乙", "y"),
            ("王小明", "官網改版"),
        ]);
        let (nc, np) = new_catalog_counts(&d, &imp);
        assert_eq!((nc, np), (1, 3));
        let (c0, p0) = (d.clients.len(), d.projects.len());
        apply(&mut d, imp);
        assert_eq!((d.clients.len() - c0, d.projects.len() - p0), (nc, np));
    }

    #[test]
    fn huge_minutes_is_an_error_not_a_hang() {
        let e = prep(
            "Asia/Taipei",
            &["2026-10-06,A,B,09:00,10:00,999999999999999,0,0,,"],
        )
        .unwrap_err();
        assert!(e.contains("檔案可能被改過"), "{e}");
    }

    #[test]
    fn minutes_overflow_is_an_error() {
        let m = i64::MAX;
        let row = format!("2026-10-06,A,B,09:00,10:00,{m},0,{m},,");
        let e = prep("Asia/Taipei", &[&row]).unwrap_err();
        assert!(e.contains("分鐘數太大，檔案可能被改過"), "{e}");
    }
}
