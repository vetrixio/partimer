use std::collections::BTreeMap;

use chrono::{DateTime, Datelike, NaiveDate, Utc};

use crate::duration::{effective_seconds, hours_decimal, round_minutes};
use crate::model::{Data, Entry, Id};
use crate::zone::{LocalZone, format_hhmm, local_date};

const CSV_HEADER: &str = "日期,客戶,專案,開始,結束,分鐘,小時,暫停分鐘,標題,內容";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filter {
    pub year: i32,
    pub month: u32,
    pub client_id: Option<Id>,
    pub project_id: Option<Id>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Summary {
    pub total_minutes: i64,
    /// Minutes per project, most first, ties by project id ascending.
    pub per_project: Vec<(Id, i64)>,
}

/// Entries whose local start date falls in the filter's month, in data order.
/// `project_id` wins over `client_id`; ids that no longer exist are ignored.
pub fn filtered_entries<'a>(data: &'a Data, f: &Filter, zone: &dyn LocalZone) -> Vec<&'a Entry> {
    let project = f
        .project_id
        .as_ref()
        .filter(|id| data.projects.iter().any(|p| &p.id == *id));
    let client = f
        .client_id
        .as_ref()
        .filter(|id| data.clients.iter().any(|c| &c.id == *id));
    data.entries
        .iter()
        .filter(|e| {
            let d = local_date(zone, e.start);
            if d.year() != f.year || d.month() != f.month {
                return false;
            }
            if let Some(pid) = project {
                return &e.project_id == pid;
            }
            if let Some(cid) = client {
                return data
                    .projects
                    .iter()
                    .any(|p| p.id == e.project_id && &p.client_id == cid);
            }
            true
        })
        .collect()
}

/// Finished entries only; each is rounded to minutes before summing.
pub fn summarize(data: &Data, f: &Filter, zone: &dyn LocalZone) -> Summary {
    let mut per: BTreeMap<&str, i64> = BTreeMap::new();
    let mut total = 0;
    for e in filtered_entries(data, f, zone) {
        let Some(end) = e.end else { continue };
        let m = round_minutes(effective_seconds(e, end));
        total += m;
        *per.entry(e.project_id.as_str()).or_insert(0) += m;
    }
    let mut per_project: Vec<(Id, i64)> =
        per.into_iter().map(|(p, m)| (p.to_string(), m)).collect();
    // BTreeMap iterates by id ascending; the stable sort keeps that for ties.
    per_project.sort_by_key(|p| std::cmp::Reverse(p.1));
    Summary {
        total_minutes: total,
        per_project,
    }
}

/// Entries grouped by local start date, newest day first, newest entry first within a day.
pub fn day_groups<'a>(
    data: &'a Data,
    f: &Filter,
    zone: &dyn LocalZone,
) -> Vec<(NaiveDate, Vec<&'a Entry>)> {
    let mut map: BTreeMap<NaiveDate, Vec<&'a Entry>> = BTreeMap::new();
    for e in filtered_entries(data, f, zone) {
        map.entry(local_date(zone, e.start)).or_default().push(e);
    }
    map.into_iter()
        .rev()
        .map(|(d, mut v)| {
            v.sort_by_key(|a| std::cmp::Reverse(a.start));
            (d, v)
        })
        .collect()
}

/// Entries started on the local day of `now`, plus the running one (even from an earlier day).
pub fn today_entries<'a>(
    data: &'a Data,
    zone: &dyn LocalZone,
    now: DateTime<Utc>,
) -> Vec<&'a Entry> {
    let today = local_date(zone, now);
    let mut v: Vec<&Entry> = data
        .entries
        .iter()
        .filter(|e| e.end.is_none() || local_date(zone, e.start) == today)
        .collect();
    v.sort_by_key(|a| std::cmp::Reverse(a.start));
    v
}

pub fn today_total_seconds(data: &Data, zone: &dyn LocalZone, now: DateTime<Utc>) -> i64 {
    today_entries(data, zone, now)
        .into_iter()
        .map(|e| effective_seconds(e, now))
        .sum()
}

/// CSV per spec section 10: BOM, CRLF, finished entries only, oldest first.
pub fn export_csv(data: &Data, f: &Filter, zone: &dyn LocalZone) -> Vec<u8> {
    let mut rows: Vec<&Entry> = filtered_entries(data, f, zone)
        .into_iter()
        .filter(|e| e.end.is_some())
        .collect();
    rows.sort_by_key(|e| e.start);

    let mut out = String::from(CSV_HEADER);
    out.push_str("\r\n");
    for e in rows {
        let Some(end) = e.end else { continue };
        let project = data.projects.iter().find(|p| p.id == e.project_id);
        let client = project.and_then(|p| data.clients.iter().find(|c| c.id == p.client_id));
        let minutes = round_minutes(effective_seconds(e, end));
        let cells = [
            local_date(zone, e.start).format("%Y-%m-%d").to_string(),
            client.map(|c| c.name.clone()).unwrap_or_default(),
            project.map(|p| p.name.clone()).unwrap_or_default(),
            format_hhmm(zone, e.start),
            format_hhmm(zone, end),
            minutes.to_string(),
            hours_decimal(minutes),
            round_minutes(e.paused_seconds).to_string(),
            e.title.clone(),
            e.content.clone(),
        ];
        let line: Vec<String> = cells.iter().map(|c| csv_field(c)).collect();
        out.push_str(&line.join(","));
        out.push_str("\r\n");
    }

    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(out.as_bytes());
    bytes
}

/// RFC 4180 quoting, plus OWASP CSV-injection defence: a cell starting with `=`, `+`, `-`,
/// `@`, TAB or CR gets a leading TAB (invisible in Excel, unlike `'`) so Excel shows it as
/// text instead of evaluating it (e.g. a "- " bullet would otherwise become `#NAME?`).
/// Neutralized cells are always quoted so the leading TAB is clearly part of the field.
fn csv_field(s: &str) -> String {
    let neutralize = s.starts_with(['=', '+', '-', '@', '\t', '\r']);
    if neutralize || s.contains([',', '"', '\r', '\n']) {
        let prefix = if neutralize { "\t" } else { "" };
        format!("\"{prefix}{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Client, Project};
    use crate::testutil::{tz, utc};

    fn e(id: &str, pid: &str, start: &str, end: Option<&str>) -> Entry {
        Entry {
            id: id.into(),
            project_id: pid.into(),
            start: utc(start),
            end: end.map(utc),
            title: String::new(),
            content: String::new(),
            paused_seconds: 0,
            pause_started_at: None,
        }
    }

    fn data() -> Data {
        let mut d = Data::default();
        for (id, name) in [("c1", "王小明"), ("c2", "乙公司")] {
            d.clients.push(Client {
                id: id.into(),
                name: name.into(),
                archived: false,
            });
        }
        for (id, cid, name, archived) in [
            ("p1", "c1", "官網改版", false),
            ("p2", "c1", "舊專案", true),
            ("p3", "c2", "App", false),
        ] {
            d.projects.push(Project {
                id: id.into(),
                client_id: cid.into(),
                name: name.into(),
                color: 0,
                archived,
            });
        }
        d
    }

    fn f(year: i32, month: u32) -> Filter {
        Filter {
            year,
            month,
            client_id: None,
            project_id: None,
        }
    }

    fn ids(v: &[&Entry]) -> Vec<String> {
        v.iter().map(|x| x.id.clone()).collect()
    }

    fn date(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn month_boundary_uses_local_start_date() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        // 2026-11-01 00:30 Taipei, but still October in UTC.
        d.entries.push(e(
            "a",
            "p1",
            "2026-10-31T16:30:00Z",
            Some("2026-10-31T17:30:00Z"),
        ));
        assert_eq!(filtered_entries(&d, &f(2026, 11), z.as_ref()).len(), 1);
        assert!(filtered_entries(&d, &f(2026, 10), z.as_ref()).is_empty());
    }

    #[test]
    fn cross_midnight_entry_belongs_to_start_day() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        // 2026-10-31 23:00 -> 2026-11-01 01:00 Taipei.
        d.entries.push(e(
            "a",
            "p1",
            "2026-10-31T15:00:00Z",
            Some("2026-10-31T17:00:00Z"),
        ));
        assert_eq!(filtered_entries(&d, &f(2026, 10), z.as_ref()).len(), 1);
        assert!(filtered_entries(&d, &f(2026, 11), z.as_ref()).is_empty());
        let g = day_groups(&d, &f(2026, 10), z.as_ref());
        assert_eq!(g[0].0, date(2026, 10, 31));
    }

    #[test]
    fn summary_rounds_each_entry_then_sums() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        d.entries.push(e(
            "a",
            "p1",
            "2026-10-06T01:00:00Z",
            Some("2026-10-06T01:01:29Z"),
        ));
        d.entries.push(e(
            "b",
            "p1",
            "2026-10-06T02:00:00Z",
            Some("2026-10-06T02:01:29Z"),
        ));
        let s = summarize(&d, &f(2026, 10), z.as_ref());
        assert_eq!(s.total_minutes, 2);
        assert_eq!(s.per_project, vec![("p1".to_string(), 2)]);
    }

    #[test]
    fn summary_excludes_running() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        d.entries.push(e(
            "a",
            "p1",
            "2026-10-06T01:00:00Z",
            Some("2026-10-06T02:00:00Z"),
        ));
        d.entries.push(e("run", "p1", "2026-10-06T03:00:00Z", None));
        let s = summarize(&d, &f(2026, 10), z.as_ref());
        assert_eq!(s.total_minutes, 60);
    }

    #[test]
    fn per_project_sorted_by_minutes_then_id() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        d.entries.push(e(
            "a",
            "p3",
            "2026-10-06T01:00:00Z",
            Some("2026-10-06T02:00:00Z"),
        ));
        d.entries.push(e(
            "b",
            "p2",
            "2026-10-06T03:00:00Z",
            Some("2026-10-06T04:00:00Z"),
        ));
        d.entries.push(e(
            "c",
            "p1",
            "2026-10-06T05:00:00Z",
            Some("2026-10-06T07:00:00Z"),
        ));
        let s = summarize(&d, &f(2026, 10), z.as_ref());
        let order: Vec<&str> = s.per_project.iter().map(|(p, _)| p.as_str()).collect();
        assert_eq!(order, vec!["p1", "p2", "p3"]);
    }

    #[test]
    fn filter_by_client_and_project() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        for (id, p) in [("a", "p1"), ("b", "p2"), ("c", "p3")] {
            d.entries.push(e(
                id,
                p,
                "2026-10-06T01:00:00Z",
                Some("2026-10-06T02:00:00Z"),
            ));
        }
        let mut fl = f(2026, 10);
        fl.client_id = Some("c1".into());
        let got = filtered_entries(&d, &fl, z.as_ref());
        let mut got = ids(&got);
        got.sort();
        assert_eq!(got, vec!["a", "b"]); // archived project included

        // project_id wins over client_id
        fl.project_id = Some("p2".into());
        assert_eq!(ids(&filtered_entries(&d, &fl, z.as_ref())), vec!["b"]);

        // unknown ids mean no filtering
        let mut fl = f(2026, 10);
        fl.client_id = Some("nope".into());
        fl.project_id = Some("nope".into());
        assert_eq!(filtered_entries(&d, &fl, z.as_ref()).len(), 3);
    }

    #[test]
    fn day_groups_newest_first() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        d.entries.push(e(
            "old",
            "p1",
            "2026-10-05T01:00:00Z",
            Some("2026-10-05T02:00:00Z"),
        ));
        d.entries.push(e(
            "am",
            "p1",
            "2026-10-06T01:00:00Z",
            Some("2026-10-06T02:00:00Z"),
        ));
        d.entries.push(e(
            "pm",
            "p1",
            "2026-10-06T05:00:00Z",
            Some("2026-10-06T06:00:00Z"),
        ));
        d.entries.push(e("run", "p1", "2026-10-07T01:00:00Z", None));
        let g = day_groups(&d, &f(2026, 10), z.as_ref());
        let days: Vec<NaiveDate> = g.iter().map(|(day, _)| *day).collect();
        assert_eq!(
            days,
            vec![date(2026, 10, 7), date(2026, 10, 6), date(2026, 10, 5)]
        );
        assert_eq!(ids(&g[0].1), vec!["run"]);
        assert_eq!(ids(&g[1].1), vec!["pm", "am"]);
    }

    #[test]
    fn today_includes_running_from_yesterday() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        let now = utc("2026-10-07T04:00:00Z"); // 12:00 Taipei on 10-07
        d.entries.push(e(
            "yday",
            "p1",
            "2026-10-06T01:00:00Z",
            Some("2026-10-06T02:00:00Z"),
        ));
        d.entries.push(e("run", "p1", "2026-10-06T14:00:00Z", None)); // 10-06 22:00 Taipei
        d.entries.push(e(
            "t1",
            "p1",
            "2026-10-07T01:00:00Z",
            Some("2026-10-07T02:00:00Z"),
        ));
        d.entries.push(e("t2", "p1", "2026-10-07T03:00:00Z", None)); // running today
        let got = today_entries(&d, z.as_ref(), now);
        assert_eq!(ids(&got), vec!["t2", "t1", "run"]);
        // run: 14:00Z..04:00Z next day = 14h; t1: 1h; t2: 1h
        assert_eq!(
            today_total_seconds(&d, z.as_ref(), now),
            (14 + 1 + 1) * 3600
        );
    }

    #[test]
    fn csv_has_bom_crlf_and_header() {
        let z = tz("Asia/Taipei");
        let out = export_csv(&data(), &f(2026, 10), z.as_ref());
        assert_eq!(&out[..3], &[0xEF, 0xBB, 0xBF]);
        let s = String::from_utf8(out[3..].to_vec()).unwrap();
        assert_eq!(
            s,
            "日期,客戶,專案,開始,結束,分鐘,小時,暫停分鐘,標題,內容\r\n"
        );
    }

    #[test]
    fn csv_example_row() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        let mut x = e(
            "a",
            "p1",
            "2026-10-06T01:30:00Z",
            Some("2026-10-06T03:00:00Z"),
        );
        x.title = "首頁切版".into();
        x.content = "改 banner".into();
        d.entries.push(x);
        let out = export_csv(&d, &f(2026, 10), z.as_ref());
        let s = String::from_utf8(out[3..].to_vec()).unwrap();
        let lines: Vec<&str> = s.split("\r\n").collect();
        assert_eq!(
            lines[1],
            "2026-10-06,王小明,官網改版,09:30,11:00,90,1.50,0,首頁切版,改 banner"
        );
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[2], "");
    }

    #[test]
    fn csv_escapes_comma_quote_newline() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        let mut x = e(
            "a",
            "p1",
            "2026-10-06T01:30:00Z",
            Some("2026-10-06T03:00:00Z"),
        );
        x.content = "a,\"b\"\nc".into();
        d.entries.push(x);
        let out = export_csv(&d, &f(2026, 10), z.as_ref());
        let s = String::from_utf8(out[3..].to_vec()).unwrap();
        assert!(s.ends_with(",\"a,\"\"b\"\"\nc\"\r\n"), "{s:?}");
        assert_eq!(csv_field("a\rb"), "\"a\rb\"");
        assert_eq!(csv_field("plain"), "plain");
    }

    #[test]
    fn csv_neutralizes_formula_like_cells() {
        // Leading = + - @ TAB CR get an invisible TAB so Excel shows text, not a formula.
        assert_eq!(csv_field("- 改 banner"), "\"\t- 改 banner\"");
        assert_eq!(csv_field("=1+1"), "\"\t=1+1\"");
        assert_eq!(csv_field("@x"), "\"\t@x\"");
        assert_eq!(csv_field("+1"), "\"\t+1\"");
        assert_eq!(csv_field("\tx"), "\"\t\tx\"");
        assert_eq!(csv_field("\rx"), "\"\t\rx\"");
        assert_eq!(csv_field("- a,\"b\""), "\"\t- a,\"\"b\"\"\"");
        assert_eq!(csv_field("a-b"), "a-b");
        assert_eq!(csv_field("1.50"), "1.50");
    }

    #[test]
    fn csv_neutralizes_every_text_column() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        d.clients[0].name = "=客戶".into();
        d.projects[0].name = "+專案".into();
        let mut x = e(
            "a",
            "p1",
            "2026-10-06T01:30:00Z",
            Some("2026-10-06T03:00:00Z"),
        );
        x.title = "@標題".into();
        x.content = "- 改 banner\n- 修 RWD".into();
        d.entries.push(x);
        let out = export_csv(&d, &f(2026, 10), z.as_ref());
        let s = String::from_utf8(out[3..].to_vec()).unwrap();
        let lines: Vec<&str> = s.split("\r\n").collect();
        assert_eq!(
            lines[1],
            "2026-10-06,\"\t=客戶\",\"\t+專案\",09:30,11:00,90,1.50,0,\"\t@標題\",\"\t- 改 banner\n- 修 RWD\""
        );
    }

    #[test]
    fn csv_sorted_oldest_first_and_skips_running() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        for (id, s, en) in [
            ("late", "2026-10-08T01:00:00Z", Some("2026-10-08T02:00:00Z")),
            ("run", "2026-10-09T01:00:00Z", None),
            (
                "early",
                "2026-10-06T01:00:00Z",
                Some("2026-10-06T02:00:00Z"),
            ),
        ] {
            let mut x = e(id, "p1", s, en);
            x.title = id.into();
            d.entries.push(x);
        }
        let out = export_csv(&d, &f(2026, 10), z.as_ref());
        let s = String::from_utf8(out[3..].to_vec()).unwrap();
        let lines: Vec<&str> = s.trim_end().split("\r\n").collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[1].contains(",early,"));
        assert!(lines[2].contains(",late,"));
    }

    #[test]
    fn csv_uses_local_time_not_utc() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        // 23:30Z on 10-05 is 07:30 Taipei on 10-06.
        d.entries.push(e(
            "a",
            "p1",
            "2026-10-05T23:30:00Z",
            Some("2026-10-06T00:15:00Z"),
        ));
        let out = export_csv(&d, &f(2026, 10), z.as_ref());
        let s = String::from_utf8(out[3..].to_vec()).unwrap();
        assert!(
            s.contains("\r\n2026-10-06,王小明,官網改版,07:30,08:15,45,0.75,0,"),
            "{s:?}"
        );
    }

    #[test]
    fn csv_pause_minutes_and_missing_project() {
        let z = tz("Asia/Taipei");
        let mut d = data();
        let mut x = e(
            "a",
            "gone",
            "2026-10-06T01:00:00Z",
            Some("2026-10-06T03:00:00Z"),
        );
        x.paused_seconds = 1200;
        d.entries.push(x);
        let out = export_csv(&d, &f(2026, 10), z.as_ref());
        let s = String::from_utf8(out[3..].to_vec()).unwrap();
        assert!(
            s.contains("\r\n2026-10-06,,,09:00,11:00,100,1.67,20,,\r\n"),
            "{s:?}"
        );
    }

    #[test]
    fn month_boundary_dst_zone() {
        let z = tz("America/New_York");
        let mut d = data();
        // DST ends 2026-11-01 02:00 local.
        d.entries.push(e(
            "oct",
            "p1",
            "2026-11-01T03:30:00Z",
            Some("2026-11-01T04:00:00Z"),
        )); // 10-31 23:30 EDT
        d.entries.push(e(
            "nov",
            "p1",
            "2026-11-01T04:30:00Z",
            Some("2026-11-01T05:00:00Z"),
        )); // 11-01 00:30 EDT
        d.entries.push(e(
            "nov2",
            "p1",
            "2026-11-01T07:30:00Z",
            Some("2026-11-01T08:00:00Z"),
        )); // 02:30 EST
        assert_eq!(
            ids(&filtered_entries(&d, &f(2026, 10), z.as_ref())),
            vec!["oct"]
        );
        assert_eq!(
            ids(&filtered_entries(&d, &f(2026, 11), z.as_ref())),
            vec!["nov", "nov2"]
        );
    }
}
