//! Developer services: GitHub, LeetCode, Linear, Resend, Supabase, Vercel.

use super::common::*;
use crate::feed::*;
use crate::http;
use serde_json::{json, Value};

pub(super) static INFOS: &[ConnectorInfo] = &[
    ConnectorInfo {
        id: "github",
        name: "GitHub",
        about: "Your public contribution graph, streak, repos, stars and followers; works with just a username (an optional fine-grained token with no extra permissions raises rate limits).",
        help_url: "https://github.com/settings/personal-access-tokens/new",
        fields: &[
            Field { key: "username", label: "Username", placeholder: "octocat", secret: false, required: true },
            Field { key: "token", label: "Token (optional)", placeholder: "github_pat_…", secret: true, required: false },
        ],
        refresh_secs: 1800,
    },
    ConnectorInfo {
        id: "leetcode",
        name: "LeetCode",
        about: "Problems solved by difficulty and your submission calendar from your public LeetCode profile; no key needed.",
        help_url: "https://leetcode.com/profile/",
        fields: &[Field { key: "username", label: "Username", placeholder: "lee215", secret: false, required: true }],
        refresh_secs: 1800,
    },
    ConnectorInfo {
        id: "linear",
        name: "Linear",
        about: "Open issues assigned to you; needs a personal API key (create it with Read-only access).",
        help_url: "https://linear.app/settings/account/security",
        fields: &[Field { key: "api_key", label: "Personal API key", placeholder: "lin_api_…", secret: true, required: true }],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "resend",
        name: "Resend",
        about: "Emails sent in the last 30 days; Resend only lists emails for a Full access API key (sending-only keys are refused).",
        help_url: "https://resend.com/api-keys",
        fields: &[Field { key: "api_key", label: "API key", placeholder: "re_…", secret: true, required: true }],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "supabase",
        name: "Supabase",
        about: "API requests to one project over the last 7 days (REST, Auth, Storage, Realtime); needs a personal access token and the project ref.",
        help_url: "https://supabase.com/dashboard/account/tokens",
        fields: &[
            Field { key: "access_token", label: "Personal access token", placeholder: "sbp_…", secret: true, required: true },
            Field { key: "project_ref", label: "Project ref", placeholder: "abcdefghijklmnopqrst", secret: false, required: true },
        ],
        refresh_secs: 900,
    },
    ConnectorInfo {
        id: "vercel",
        name: "Vercel",
        about: "Deployments in the last 7 days and the latest ones' status; needs an access token (scope it to the one team if you can).",
        help_url: "https://vercel.com/account/settings/tokens",
        fields: &[
            Field { key: "token", label: "Access token", placeholder: "…", secret: true, required: true },
            Field { key: "team_id", label: "Team ID (optional)", placeholder: "team_…", secret: false, required: false },
        ],
        refresh_secs: 900,
    },
];

/// Current streak: consecutive active days ending on the last day, or on the day before
/// it when the last day (today) has nothing yet.
fn streak(levels: &[f32]) -> usize {
    let mut v = levels;
    if v.last() == Some(&0.0) {
        v = &v[..v.len() - 1];
    }
    v.iter().rev().take_while(|&&l| l > 0.0).count()
}

// ---------------------------------------------------------------- GitHub

fn level_frac(l: u32) -> f32 {
    (l.min(4) as f32) / 4.0
}

pub fn github(p: &Params) -> Result<Feed, FeedError> {
    let user = need(p, "username", "GitHub username")?;
    if !user.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(FeedError::NeedsSetup("Add your GitHub username (letters, digits and dashes)".into()));
    }
    let auth = opt(p, "token").map(|t| format!("Bearer {t}"));
    let mut h: Vec<(&str, &str)> = vec![("Accept", "application/vnd.github+json")];
    if let Some(a) = &auth {
        h.push(("Authorization", a));
    }
    let (days, total) = if auth.is_some() {
        let q = json!({ "query": "query($u:String!){user(login:$u){contributionsCollection{contributionCalendar{totalContributions weeks{contributionDays{date contributionLevel}}}}}}", "variables": { "u": user } });
        let v = post_json("https://api.github.com/graphql", &h, &q, "GitHub")?;
        graphql_error(&v, "GitHub")?;
        parse_github_graphql(&v)
    } else {
        let r = http::get(&format!("https://github.com/users/{user}/contributions"), &[("Accept", "text/html")]).map_err(|e| net("GitHub", e))?;
        if r.status == 404 {
            return Err(FeedError::NeedsSetup(format!("GitHub has no user named {user}")));
        }
        check(&r, "GitHub")?;
        parse_github_html(&r.text())
    };
    if days.is_empty() {
        return Err(FeedError::Failed("GitHub: couldn't read the contribution graph".into()));
    }
    let grid: Vec<f32> = days.iter().map(|d| d.1).collect();
    let mut f = Feed { value: Some(total as f64), unit: "contributions".into(), caption: "Last year".into(), ..Default::default() };
    f.extra.insert("streak".into(), streak(&grid).to_string());
    f.grid = grid;
    // Profile numbers (best effort: the graph is the main thing).
    if let Ok(u) = get_json(&format!("https://api.github.com/users/{user}"), &h, "GitHub") {
        let repos = num0(&u["public_repos"]);
        let followers = num0(&u["followers"]);
        let stars = get_json(&format!("https://api.github.com/users/{user}/repos?per_page=100&type=owner"), &h, "GitHub").ok().map(|v| github_stars(&v));
        f.extra.insert("repos".into(), group(repos, 0));
        f.extra.insert("followers".into(), compact(followers));
        f.rows.push(FeedRow { label: "Repos".into(), value: group(repos, 0), ..Default::default() });
        if let Some(s) = stars {
            f.extra.insert("stars".into(), compact(s));
            f.rows.push(FeedRow { label: "Stars".into(), value: compact(s), ..Default::default() });
        }
        f.rows.push(FeedRow { label: "Followers".into(), value: compact(followers), ..Default::default() });
    }
    Ok(f)
}

/// The value of `name="…"` inside one tag's text.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pat = format!("{name}=\"");
    let i = tag.find(&pat)? + pat.len();
    let j = tag[i..].find('"')?;
    Some(&tag[i..i + j])
}

/// Public contributions page → (date, level 0..1) oldest first, and the year's total
/// (from the "N contributions in the last year" heading, else summed from tooltips).
fn parse_github_html(html: &str) -> (Vec<(String, f32)>, u64) {
    let mut days: Vec<(String, f32)> = Vec::new();
    let mut tip_sum = 0u64;
    for tag in html.split('<') {
        if let (Some(d), Some(l)) = (attr(tag, "data-date"), attr(tag, "data-level")) {
            days.push((d.to_string(), level_frac(l.parse().unwrap_or(0))));
        } else if tag.starts_with("tool-tip") {
            // "tool-tip for=… >3 contributions on November 16th."
            let text = tag.split_once('>').map(|x| x.1).unwrap_or("");
            let n: String = text.trim_start().chars().take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| *c != ',').collect();
            tip_sum += n.parse::<u64>().unwrap_or(0);
        }
    }
    days.sort_by(|a, b| a.0.cmp(&b.0));
    days.dedup_by(|a, b| a.0 == b.0);
    let heading = html
        .find("js-contribution-activity-description")
        .and_then(|i| {
            let rest = &html[i..];
            let start = rest.find('>')? + 1;
            let end = rest.find("</h2>")?;
            let text: String = rest.get(start..end)?.split_whitespace().collect::<Vec<_>>().join(" ");
            let n: String = text.chars().take_while(|c| c.is_ascii_digit() || *c == ',').filter(|c| *c != ',').collect();
            n.parse::<u64>().ok()
        });
    (days, heading.unwrap_or(tip_sum))
}

fn parse_github_graphql(v: &Value) -> (Vec<(String, f32)>, u64) {
    let cal = &v["data"]["user"]["contributionsCollection"]["contributionCalendar"];
    let mut days = Vec::new();
    for w in cal["weeks"].as_array().into_iter().flatten() {
        for d in w["contributionDays"].as_array().into_iter().flatten() {
            let l = match d["contributionLevel"].as_str() {
                Some("FIRST_QUARTILE") => 1,
                Some("SECOND_QUARTILE") => 2,
                Some("THIRD_QUARTILE") => 3,
                Some("FOURTH_QUARTILE") => 4,
                _ => 0,
            };
            days.push((d["date"].as_str().unwrap_or("").to_string(), level_frac(l)));
        }
    }
    days.sort_by(|a, b| a.0.cmp(&b.0));
    (days, cal["totalContributions"].as_u64().unwrap_or(0))
}

fn github_stars(repos: &Value) -> f64 {
    repos.as_array().into_iter().flatten().map(|r| num0(&r["stargazers_count"])).sum()
}

// ---------------------------------------------------------------- LeetCode

const LEETCODE_QUERY: &str = "query($u:String!){matchedUser(username:$u){submitStatsGlobal{acSubmissionNum{difficulty count}} userCalendar{streak totalActiveDays submissionCalendar}}}";

pub fn leetcode(p: &Params) -> Result<Feed, FeedError> {
    let user = need(p, "username", "LeetCode username")?;
    let body = json!({ "query": LEETCODE_QUERY, "variables": { "u": user } });
    let v = post_json("https://leetcode.com/graphql", &[("Referer", "https://leetcode.com/"), ("Accept", "application/json")], &body, "LeetCode")?;
    if v["data"]["matchedUser"].is_null() {
        graphql_error(&v, "LeetCode")?;
        return Err(FeedError::NeedsSetup(format!("LeetCode has no user named {user}")));
    }
    Ok(parse_leetcode(&v, now()))
}

/// Solved counts + a 365-day submission grid (UTC days, oldest first). Intensity is
/// quartiles of the busiest day, like GitHub's levels.
fn parse_leetcode(v: &Value, now: i64) -> Feed {
    let u = &v["data"]["matchedUser"];
    let mut f = Feed { unit: "solved".into(), caption: "Problems solved".into(), ..Default::default() };
    for s in u["submitStatsGlobal"]["acSubmissionNum"].as_array().into_iter().flatten() {
        let n = num0(&s["count"]);
        match s["difficulty"].as_str() {
            Some("All") => f.value = Some(n),
            Some(d) => f.rows.push(FeedRow { label: d.to_string(), value: group(n, 0), ..Default::default() }),
            None => {}
        }
    }
    let total = f.value.unwrap_or(0.0);
    for r in &mut f.rows {
        r.frac = (total > 0.0).then(|| r.value.replace(',', "").parse::<f64>().unwrap_or(0.0) as f32 / total as f32);
    }
    let cal = &u["userCalendar"];
    let counts: serde_json::Map<String, Value> = cal["submissionCalendar"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default();
    let start = day_start(now) - 364 * DAY;
    let mut days = vec![0.0f64; 365];
    for (k, c) in &counts {
        if let Ok(t) = k.parse::<i64>() {
            if t >= start {
                if let Some(d) = days.get_mut(((t - start) / DAY) as usize) {
                    *d += num0(c);
                }
            }
        }
    }
    let max = days.iter().cloned().fold(0.0, f64::max);
    f.grid = days
        .iter()
        .map(|&c| {
            if c <= 0.0 || max <= 0.0 {
                0.0
            } else {
                ((c / max * 4.0).ceil() / 4.0) as f32
            }
        })
        .collect();
    f.extra.insert("streak".into(), streak(&f.grid).to_string());
    if let Some(s) = cal["streak"].as_u64() {
        f.extra.insert("max_streak".into(), s.to_string());
    }
    if let Some(s) = cal["totalActiveDays"].as_u64() {
        f.extra.insert("active_days".into(), s.to_string());
    }
    f
}

// ---------------------------------------------------------------- Linear

const LINEAR_QUERY: &str = r#"query{viewer{assignedIssues(first:100,orderBy:updatedAt,filter:{state:{type:{nin:["completed","canceled"]}}}){nodes{identifier title state{name}} pageInfo{hasNextPage}}}}"#;

pub fn linear(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "api_key", "Linear personal API key")?;
    let v = post_json("https://api.linear.app/graphql", &[("Authorization", key)], &json!({ "query": LINEAR_QUERY }), "Linear")?;
    graphql_error(&v, "Linear")?;
    Ok(parse_linear(&v))
}

fn parse_linear(v: &Value) -> Feed {
    let a = &v["data"]["viewer"]["assignedIssues"];
    let nodes = a["nodes"].as_array().cloned().unwrap_or_default();
    let mut f = Feed { value: Some(nodes.len() as f64), unit: "issues".into(), caption: "Assigned to you".into(), ..Default::default() };
    if a["pageInfo"]["hasNextPage"].as_bool() == Some(true) {
        f.text = Some(format!("{}+", nodes.len()));
    }
    f.rows = nodes
        .iter()
        .take(4)
        .map(|n| FeedRow {
            label: format!("{} {}", n["identifier"].as_str().unwrap_or(""), n["title"].as_str().unwrap_or("")).trim().to_string(),
            value: n["state"]["name"].as_str().unwrap_or("").to_string(),
            ..Default::default()
        })
        .collect();
    f
}

// ---------------------------------------------------------------- Resend

pub fn resend(p: &Params) -> Result<Feed, FeedError> {
    let key = need(p, "api_key", "Resend API key")?;
    let auth = format!("Bearer {key}");
    let now = now();
    let start = day_start(now) - 29 * DAY;
    let mut pages = Vec::new();
    let mut after: Option<String> = None;
    let mut capped = true;
    for _ in 0..10 {
        let mut url = "https://api.resend.com/emails?limit=100".to_string();
        if let Some(a) = &after {
            url.push_str(&format!("&after={}", http::encode(a)));
        }
        let v = get_json(&url, &[("Authorization", auth.as_str())], "Resend")?;
        let data = v["data"].as_array().cloned().unwrap_or_default();
        let oldest = data.iter().filter_map(|e| e["created_at"].as_str().and_then(parse_time)).min();
        after = data.last().and_then(|e| e["id"].as_str()).map(String::from);
        let more = v["has_more"].as_bool() == Some(true);
        pages.push(v);
        if !more || after.is_none() || oldest.is_none_or(|t| t < start) {
            capped = false;
            break;
        }
    }
    Ok(parse_resend(&pages, now, capped))
}

fn parse_resend(pages: &[Value], now: i64, capped: bool) -> Feed {
    let pts: Vec<(i64, f64)> = pages
        .iter()
        .flat_map(|v| v["data"].as_array().cloned().unwrap_or_default())
        .filter_map(|e| Some((e["created_at"].as_str().and_then(parse_time)?, 1.0)))
        .collect();
    let (series, _) = daily(&pts, 30, now);
    let n: f64 = series.iter().sum();
    Feed { value: Some(n), text: capped.then(|| format!("{}+", n as u64)), unit: "emails".into(), caption: "Emails sent".into(), series, ..Default::default() }
}

// ---------------------------------------------------------------- Supabase

pub fn supabase(p: &Params) -> Result<Feed, FeedError> {
    let token = need(p, "access_token", "Supabase personal access token")?;
    let project = need(p, "project_ref", "Supabase project ref")?;
    let auth = format!("Bearer {token}");
    let url = format!("https://api.supabase.com/v1/projects/{}/analytics/endpoints/usage.api-counts?interval=7day", http::encode(project));
    let v = get_json(&url, &[("Authorization", auth.as_str())], "Supabase")?;
    parse_supabase(&v)
}

/// A timestamp as ISO text, or a number in seconds/ms/µs.
fn any_time(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => parse_time(s),
        _ => {
            let n = num(v)?;
            Some(if n > 1e14 { n / 1e6 } else if n > 1e11 { n / 1e3 } else { n } as i64)
        }
    }
}

fn parse_supabase(v: &Value) -> Result<Feed, FeedError> {
    let mut res: Vec<&Value> = v["result"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
    if res.is_empty() && !v["error"].is_null() {
        return Err(FeedError::Failed("Supabase: usage isn't available for this project".into()));
    }
    res.sort_by_key(|r| any_time(&r["timestamp"]).unwrap_or(0));
    let kinds = [("REST", "total_rest_requests"), ("Auth", "total_auth_requests"), ("Storage", "total_storage_requests"), ("Realtime", "total_realtime_requests")];
    let mut sums = [0.0; 4];
    let mut series = Vec::new();
    for r in &res {
        let mut t = 0.0;
        for (i, (_, k)) in kinds.iter().enumerate() {
            let x = num0(&r[*k]);
            sums[i] += x;
            t += x;
        }
        series.push(t);
    }
    let total: f64 = sums.iter().sum();
    let items = kinds.iter().zip(sums).map(|((l, _), s)| (l.to_string(), s)).collect();
    Ok(Feed { value: Some(total), unit: "requests".into(), caption: "API requests, 7 days".into(), series, rows: top_rows(items, 4, total, compact), ..Default::default() })
}

// ---------------------------------------------------------------- Vercel

pub fn vercel(p: &Params) -> Result<Feed, FeedError> {
    let token = need(p, "token", "Vercel access token")?;
    let auth = format!("Bearer {token}");
    let mut url = "https://api.vercel.com/v6/deployments?limit=100".to_string();
    if let Some(t) = opt(p, "team_id") {
        url.push_str(&format!("&teamId={}", http::encode(t)));
    }
    let v = get_json(&url, &[("Authorization", auth.as_str())], "Vercel")?;
    Ok(parse_vercel(&v, now()))
}

/// "READY" → "Ready".
fn title_case(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut c = lower.chars();
    c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

fn parse_vercel(v: &Value, now: i64) -> Feed {
    let mut deps: Vec<(i64, &str, String)> = v["deployments"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|d| {
            let ms = num(&d["created"]).or(num(&d["createdAt"])).unwrap_or(0.0);
            let state = d["state"].as_str().or(d["readyState"].as_str()).unwrap_or("");
            ((ms / 1000.0) as i64, d["name"].as_str().unwrap_or("?"), title_case(state))
        })
        .collect();
    deps.sort_by(|a, b| b.0.cmp(&a.0));
    let pts: Vec<(i64, f64)> = deps.iter().map(|d| (d.0, 1.0)).collect();
    let (series, _) = daily(&pts, 7, now);
    let mut f = Feed {
        value: Some(series.iter().sum()),
        unit: "deployments".into(),
        caption: "Deployments, 7 days".into(),
        series,
        rows: deps.iter().take(4).map(|d| FeedRow { label: d.1.to_string(), value: d.2.clone(), ..Default::default() }).collect(),
        ..Default::default()
    };
    if let Some(d) = deps.first() {
        f.extra.insert("latest_state".into(), d.2.clone());
        f.extra.insert("latest_project".into(), d.1.to_string());
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    fn now() -> i64 {
        parse_time("2026-11-14T12:00:00Z").unwrap()
    }
    #[test]
    fn github_html() {
        let html = r#"<h2 id="js-contribution-activity-description" class="f4 text-normal mb-2">
              1,234
              contributions
                in the last year
            </h2>
            <tr><td tabindex="0" data-ix="0" aria-selected="false" data-date="2026-11-13" id="contribution-day-component-5-52" data-level="2" role="gridcell" class="ContributionCalendar-day"></td>
            <tool-tip for="contribution-day-component-5-52" popover="manual">5 contributions on November 13th.</tool-tip>
            <td data-date="2026-11-12" id="c-4-52" data-level="4" class="ContributionCalendar-day"></td>
            <tool-tip for="c-4-52">12 contributions on November 12th.</tool-tip>
            <td data-date="2026-11-14" id="c-6-52" data-level="0" class="ContributionCalendar-day"></td>
            <tool-tip for="c-6-52">No contributions on November 14th.</tool-tip>
            <td data-date="2026-11-11" id="c-3-52" data-level="0"></td></tr>"#;
        let (days, total) = parse_github_html(html);
        assert_eq!(total, 1234);
        let grid: Vec<f32> = days.iter().map(|d| d.1).collect();
        assert_eq!(days[0].0, "2026-11-11");
        assert_eq!(grid, vec![0.0, 1.0, 0.5, 0.0]);
        assert_eq!(streak(&grid), 2);
        let (_, total) = parse_github_html(&html.replace("js-contribution-activity-description", "x"));
        assert_eq!(total, 17);
        let repos: Value = serde_json::from_str(r#"[{"stargazers_count":10},{"stargazers_count":5}]"#).unwrap();
        assert_eq!(github_stars(&repos), 15.0);
    }
    #[test]
    fn github_graphql() {
        let v: Value = serde_json::from_str(
            r#"{"data":{"user":{"contributionsCollection":{"contributionCalendar":{"totalContributions":42,"weeks":[
              {"contributionDays":[{"date":"2026-11-12","contributionLevel":"FOURTH_QUARTILE"},{"date":"2026-11-13","contributionLevel":"NONE"}]}]}}}}}"#,
        )
        .unwrap();
        let (days, total) = parse_github_graphql(&v);
        assert_eq!(total, 42);
        assert_eq!(days[0].1, 1.0);
    }
    #[test]
    fn leetcode_profile() {
        let today = day_start(now());
        let cal = format!("{{\"{}\": 4, \"{}\": 1, \"{}\": 2}}", today, today - DAY, today - 400 * DAY);
        let v = json!({"data":{"matchedUser":{"submitStatsGlobal":{"acSubmissionNum":[
            {"difficulty":"All","count":100},{"difficulty":"Easy","count":50},{"difficulty":"Medium","count":40},{"difficulty":"Hard","count":10}]},
            "userCalendar":{"streak":9,"totalActiveDays":120,"submissionCalendar":cal}}}});
        let f = parse_leetcode(&v, now());
        assert_eq!(f.value, Some(100.0));
        assert_eq!(f.rows.len(), 3);
        assert_eq!(f.rows[2].label, "Hard");
        assert_eq!(f.rows[0].frac, Some(0.5));
        assert_eq!(f.grid.len(), 365);
        assert_eq!(f.grid[364], 1.0);
        assert_eq!(f.grid[363], 0.25);
        assert_eq!(f.extra["streak"], "2");
        assert_eq!(f.extra["max_streak"], "9");
    }
    #[test]
    fn linear_issues() {
        let v: Value = serde_json::from_str(
            r#"{"data":{"viewer":{"assignedIssues":{"nodes":[
              {"identifier":"ENG-1","title":"Fix login","state":{"name":"In Progress"}},
              {"identifier":"ENG-2","title":"Docs","state":{"name":"Todo"}}],"pageInfo":{"hasNextPage":false}}}}}"#,
        )
        .unwrap();
        let f = parse_linear(&v);
        assert_eq!(f.value, Some(2.0));
        assert_eq!(f.rows[0].label, "ENG-1 Fix login");
        assert_eq!(f.rows[0].value, "In Progress");
        assert!(graphql_error(&serde_json::from_str(r#"{"errors":[{"message":"Authentication required"}]}"#).unwrap(), "Linear").is_err());
    }
    #[test]
    fn resend_emails() {
        let v: Value = serde_json::from_str(
            r#"{"object":"list","has_more":false,"data":[
              {"id":"a","to":["x@y.com"],"created_at":"2026-11-14 09:13:42.674981+00","subject":"Hi"},
              {"id":"b","created_at":"2026-11-13 22:13:42.674981+00"},
              {"id":"c","created_at":"2026-09-01 22:13:42.674981+00"}]}"#,
        )
        .unwrap();
        let f = parse_resend(&[v], now(), false);
        assert_eq!(f.value, Some(2.0));
        assert_eq!(f.series.len(), 30);
        assert_eq!(f.text, None);
    }
    #[test]
    fn supabase_counts() {
        let v: Value = serde_json::from_str(
            r#"{"result":[
              {"timestamp":"2026-11-14T00:00:00Z","total_auth_requests":5,"total_realtime_requests":0,"total_rest_requests":100,"total_storage_requests":2},
              {"timestamp":"2026-11-13T00:00:00Z","total_auth_requests":1,"total_realtime_requests":1,"total_rest_requests":50,"total_storage_requests":0}],"error":null}"#,
        )
        .unwrap();
        let f = parse_supabase(&v).unwrap();
        assert_eq!(f.value, Some(159.0));
        assert_eq!(f.series, vec![52.0, 107.0]);
        assert_eq!(f.rows[0].label, "REST");
        assert_eq!(any_time(&json!(1763078400000000i64)), Some(1763078400));
        assert!(parse_supabase(&json!({"result":[],"error":"boom"})).is_err());
    }
    #[test]
    fn vercel_deployments() {
        let n = now() * 1000;
        let v = json!({"deployments":[
            {"uid":"1","name":"site","state":"READY","created": n - 3_600_000},
            {"uid":"2","name":"api","state":"ERROR","created": n - 1_000},
            {"uid":"3","name":"old","readyState":"READY","created": n - 10 * 86_400_000}],"pagination":{}});
        let f = parse_vercel(&v, now());
        assert_eq!(f.value, Some(2.0));
        assert_eq!(f.rows[0].label, "api");
        assert_eq!(f.extra["latest_state"], "Error");
        assert_eq!(f.rows.len(), 3);
    }
}
