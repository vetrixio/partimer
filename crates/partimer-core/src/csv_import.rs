//! Parse CSV files written by `report::export_csv` back into rows (exact inverse of its format).

use std::fmt;

use chrono::{NaiveDate, NaiveTime};

use crate::report::CSV_HEADER;

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
            Some(line) => write!(f, "`{}` 第 {line} 行：{}", self.file, self.problem),
            None => write!(f, "`{}`：{}", self.file, self.problem),
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
            "`a.csv`：第一行不是打工人計時器匯出的欄位名稱"
        );
    }

    #[test]
    fn rejects_non_utf8() {
        // "日期" in Big5.
        let errs = err_text(parse_file("a.csv", &[0xA4, 0xE9, 0xB4, 0xC1, 0x0D, 0x0A]));
        assert_eq!(
            errs,
            vec!["`a.csv`：檔案不是 UTF-8 編碼，可能被 Excel 另存過"]
        );
    }

    #[test]
    fn rejects_slash_date_with_file_and_line() {
        let bytes = text(
            "2026-10-05,A,P,09:00,10:00,60,1.00,0,t,c\r\n2026/10/6,A,P,09:00,10:00,60,1.00,0,t,c\r\n",
        );
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["`a.csv` 第 3 行：日期『2026/10/6』格式不對，應為 YYYY-MM-DD"]
        );
    }

    #[test]
    fn rejects_single_digit_time() {
        let bytes = text("2026-10-05,A,P,9:3,10:00,60,1.00,0,t,c\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["`a.csv` 第 2 行：開始時間『9:3』格式不對，應為 HH:MM"]
        );
        let bytes = text("2026-10-05,A,P,09:00,10:7,60,1.00,0,t,c\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["`a.csv` 第 2 行：結束時間『10:7』格式不對，應為 HH:MM"]
        );
    }

    #[test]
    fn rejects_wrong_column_count() {
        let bytes = text("2026-10-05,A,P,09:00\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["`a.csv` 第 2 行：欄位數是 4，應為 10"]
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
                "`a.csv` 第 2 行：分鐘『-5』應為 0 以上的整數",
                "`a.csv` 第 3 行：暫停分鐘『x』應為 0 以上的整數",
            ]
        );
    }

    #[test]
    fn rejects_unterminated_quote() {
        let bytes = text("2026-10-05,A,P,09:00,10:00,60,1.00,0,\"oops,c\r\n");
        assert_eq!(
            err_text(parse_file("a.csv", &bytes)),
            vec!["`a.csv` 第 2 行：引號沒有成對"]
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
}
