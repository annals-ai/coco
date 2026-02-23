//! Currency conversion parsing and calculation.
//!
//! Supports patterns like:
//! - `100 usd to cny`  /  `100 usd in cny`  /  `100 usd cny`
//! - `$100 to cny`  /  `¥100`  /  `€50 gbp`  /  `£30`
//! - `100$`  /  `100¥`  /  `100€`  /  `100£`

use std::{
    collections::HashMap,
    process::Command,
    sync::RwLock,
    time::{Duration, SystemTime},
};

use once_cell::sync::Lazy;

/// A currency definition.
#[derive(Debug, Clone)]
pub struct CurrencyDef {
    pub code: &'static str,
    pub symbol: &'static str,
    pub name_cn: &'static str,
    pub flag: &'static str,
}

/// The result of a currency conversion.
#[derive(Debug, Clone)]
pub struct CurrencyResult {
    pub source_value: f64,
    pub source_code: &'static str,
    pub target_value: f64,
    pub target_code: &'static str,
    pub rate: f64,
    pub updated_at: String,
}

struct ParsedCurrencyQuery {
    value: f64,
    source: &'static CurrencyDef,
    target: Option<&'static CurrencyDef>,
}

// ── Currency definitions ────────────────────────────────────────────────────

const CURRENCIES: &[CurrencyDef] = &[
    CurrencyDef { code: "USD", symbol: "$",  flag: "🇺🇸", name_cn: "美元" },
    CurrencyDef { code: "CNY", symbol: "¥",  flag: "🇨🇳", name_cn: "人民币" },
    CurrencyDef { code: "EUR", symbol: "€",  flag: "🇪🇺", name_cn: "欧元" },
    CurrencyDef { code: "GBP", symbol: "£",  flag: "🇬🇧", name_cn: "英镑" },
    CurrencyDef { code: "JPY", symbol: "",   flag: "🇯🇵", name_cn: "日元" },
    CurrencyDef { code: "KRW", symbol: "₩",  flag: "🇰🇷", name_cn: "韩元" },
    CurrencyDef { code: "HKD", symbol: "",   flag: "🇭🇰", name_cn: "港币" },
    CurrencyDef { code: "TWD", symbol: "",   flag: "🇹🇼", name_cn: "新台币" },
    CurrencyDef { code: "SGD", symbol: "",   flag: "🇸🇬", name_cn: "新加坡元" },
    CurrencyDef { code: "AUD", symbol: "",   flag: "🇦🇺", name_cn: "澳元" },
    CurrencyDef { code: "CAD", symbol: "",   flag: "🇨🇦", name_cn: "加元" },
    CurrencyDef { code: "CHF", symbol: "",   flag: "🇨🇭", name_cn: "瑞士法郎" },
    CurrencyDef { code: "RUB", symbol: "₽",  flag: "🇷🇺", name_cn: "卢布" },
    CurrencyDef { code: "INR", symbol: "₹",  flag: "🇮🇳", name_cn: "印度卢比" },
    CurrencyDef { code: "BRL", symbol: "",   flag: "🇧🇷", name_cn: "巴西雷亚尔" },
    CurrencyDef { code: "MXN", symbol: "",   flag: "🇲🇽", name_cn: "墨西哥比索" },
    CurrencyDef { code: "THB", symbol: "฿",  flag: "🇹🇭", name_cn: "泰铢" },
    CurrencyDef { code: "VND", symbol: "₫",  flag: "🇻🇳", name_cn: "越南盾" },
    CurrencyDef { code: "PHP", symbol: "₱",  flag: "🇵🇭", name_cn: "菲律宾比索" },
    CurrencyDef { code: "MYR", symbol: "",   flag: "🇲🇾", name_cn: "马来西亚林吉特" },
    CurrencyDef { code: "NZD", symbol: "",   flag: "🇳🇿", name_cn: "新西兰元" },
    CurrencyDef { code: "SEK", symbol: "",   flag: "🇸🇪", name_cn: "瑞典克朗" },
    CurrencyDef { code: "NOK", symbol: "",   flag: "🇳🇴", name_cn: "挪威克朗" },
    CurrencyDef { code: "DKK", symbol: "",   flag: "🇩🇰", name_cn: "丹麦克朗" },
    CurrencyDef { code: "PLN", symbol: "zł", flag: "🇵🇱", name_cn: "波兰兹罗提" },
    CurrencyDef { code: "TRY", symbol: "₺",  flag: "🇹🇷", name_cn: "土耳其里拉" },
    CurrencyDef { code: "ZAR", symbol: "",   flag: "🇿🇦", name_cn: "南非兰特" },
    CurrencyDef { code: "AED", symbol: "",   flag: "🇦🇪", name_cn: "阿联酋迪拉姆" },
    CurrencyDef { code: "SAR", symbol: "",   flag: "🇸🇦", name_cn: "沙特里亚尔" },
];

/// Symbols that map to a specific currency.
/// Ordered so that more specific matches are attempted first.
const SYMBOL_MAP: &[(&str, &str)] = &[
    ("$", "USD"),
    ("¥", "CNY"),
    ("￥", "CNY"), // fullwidth yen
    ("€", "EUR"),
    ("£", "GBP"),
    ("₩", "KRW"),
    ("₽", "RUB"),
    ("₹", "INR"),
    ("฿", "THB"),
    ("₫", "VND"),
    ("₱", "PHP"),
    ("₺", "TRY"),
];

/// Default currencies to show when no target is specified.
const DEFAULT_TARGETS: &[&str] = &["CNY", "USD", "EUR", "GBP", "JPY", "KRW", "HKD", "CAD"];

// ── Default exchange rates (base: USD) ──────────────────────────────────────

fn default_rates() -> HashMap<String, f64> {
    let mut m = HashMap::new();
    m.insert("USD".into(), 1.0);
    m.insert("CNY".into(), 7.25);
    m.insert("EUR".into(), 0.92);
    m.insert("GBP".into(), 0.79);
    m.insert("JPY".into(), 149.5);
    m.insert("KRW".into(), 1330.0);
    m.insert("HKD".into(), 7.82);
    m.insert("TWD".into(), 31.5);
    m.insert("SGD".into(), 1.34);
    m.insert("AUD".into(), 1.53);
    m.insert("CAD".into(), 1.36);
    m.insert("CHF".into(), 0.88);
    m.insert("RUB".into(), 92.0);
    m.insert("INR".into(), 83.0);
    m.insert("BRL".into(), 4.97);
    m.insert("MXN".into(), 17.15);
    m.insert("THB".into(), 35.5);
    m.insert("VND".into(), 24_500.0);
    m.insert("PHP".into(), 56.0);
    m.insert("MYR".into(), 4.72);
    m.insert("NZD".into(), 1.63);
    m.insert("SEK".into(), 10.4);
    m.insert("NOK".into(), 10.5);
    m.insert("DKK".into(), 6.87);
    m.insert("PLN".into(), 4.0);
    m.insert("TRY".into(), 30.5);
    m.insert("ZAR".into(), 18.7);
    m.insert("AED".into(), 3.67);
    m.insert("SAR".into(), 3.75);
    m
}

// ── Global rate cache ───────────────────────────────────────────────────────

struct RateCache {
    rates: HashMap<String, f64>,
    last_updated: String,
    last_fetch: Option<SystemTime>,
}

static RATE_CACHE: Lazy<RwLock<RateCache>> = Lazy::new(|| {
    RwLock::new(RateCache {
        rates: default_rates(),
        last_updated: "默认汇率".into(),
        last_fetch: None,
    })
});

// ── Public API ──────────────────────────────────────────────────────────────

/// Parse a query string and return currency conversion results.
pub fn convert_query(query: &str) -> Option<Vec<CurrencyResult>> {
    let parsed = parse_currency_query(query)?;
    let cache = RATE_CACHE.read().ok()?;

    let source_rate = *cache.rates.get(parsed.source.code)?;

    let targets: Vec<&CurrencyDef> = match parsed.target {
        Some(t) => vec![t],
        None => DEFAULT_TARGETS
            .iter()
            .filter_map(|code| find_currency_by_code(code))
            .filter(|c| c.code != parsed.source.code)
            .collect(),
    };

    let mut results = Vec::new();
    for target in targets {
        let target_rate = match cache.rates.get(target.code) {
            Some(r) => *r,
            None => continue,
        };
        let rate = target_rate / source_rate;
        let target_value = parsed.value * rate;
        results.push(CurrencyResult {
            source_value: parsed.value,
            source_code: parsed.source.code,
            target_value,
            target_code: target.code,
            rate,
            updated_at: cache.last_updated.clone(),
        });
    }

    if results.is_empty() { None } else { Some(results) }
}

/// Format a currency value for display.
pub fn format_currency(value: f64, code: &str) -> String {
    // Zero-decimal currencies
    if matches!(code, "JPY" | "KRW" | "VND") {
        return format_with_commas(value.round() as i64);
    }
    let rounded = (value * 100.0).round() / 100.0;
    let int_part = rounded.trunc() as i64;
    let frac = ((rounded - rounded.trunc()).abs() * 100.0).round() as u32;
    format!("{}.{:02}", format_with_commas(int_part), frac)
}

/// Spawn a background thread to fetch fresh exchange rates.
pub fn spawn_rate_updater() {
    std::thread::spawn(|| {
        loop {
            fetch_rates_once();
            std::thread::sleep(Duration::from_secs(3600));
        }
    });
}

/// Get the currency name in Chinese for a given code.
pub fn currency_name_cn(code: &str) -> &str {
    find_currency_by_code(code).map_or(code, |c| c.name_cn)
}

/// Get the symbol for a given code (empty string if none).
pub fn currency_symbol(code: &str) -> &'static str {
    find_currency_by_code(code).map_or("", |c| c.symbol)
}

/// Get the flag emoji for a given code (empty string if none).
pub fn currency_flag(code: &str) -> &'static str {
    find_currency_by_code(code).map_or("", |c| c.flag)
}

// ── Parsing ─────────────────────────────────────────────────────────────────

fn parse_currency_query(query: &str) -> Option<ParsedCurrencyQuery> {
    let q = query.trim();
    if q.is_empty() {
        return None;
    }

    // Try symbol-prefixed: `$100`, `¥50.5`, `€200 gbp`
    if let Some(parsed) = try_parse_symbol_prefix(q) {
        return Some(parsed);
    }

    // Try symbol-suffixed: `100$`, `50.5¥`, `200€ gbp`
    if let Some(parsed) = try_parse_symbol_suffix(q) {
        return Some(parsed);
    }

    // Try code-based: `100 usd to cny`, `100 usd cny`, `100 usd`
    try_parse_code_based(q)
}

fn try_parse_symbol_prefix(q: &str) -> Option<ParsedCurrencyQuery> {
    for &(symbol, code) in SYMBOL_MAP {
        if !q.starts_with(symbol) {
            continue;
        }
        let rest = &q[symbol.len()..];
        let (value, after_num) = parse_number_prefix(rest)?;
        if value == 0.0 {
            return None;
        }
        let source = find_currency_by_code(code)?;
        let target = parse_optional_target(after_num);
        return Some(ParsedCurrencyQuery { value, source, target });
    }
    None
}

fn try_parse_symbol_suffix(q: &str) -> Option<ParsedCurrencyQuery> {
    let (value, rest) = parse_number_prefix(q)?;
    if value == 0.0 {
        return None;
    }
    let rest = rest.trim_start();
    for &(symbol, code) in SYMBOL_MAP {
        if !rest.starts_with(symbol) {
            continue;
        }
        let after_symbol = rest[symbol.len()..].trim_start();
        let source = find_currency_by_code(code)?;
        let target = parse_optional_target(after_symbol);
        return Some(ParsedCurrencyQuery { value, source, target });
    }
    None
}

fn try_parse_code_based(q: &str) -> Option<ParsedCurrencyQuery> {
    let (value, rest) = parse_number_prefix(q)?;
    if value == 0.0 {
        return None;
    }
    let rest = rest.trim_start();
    if rest.is_empty() {
        return None;
    }

    let tokens: Vec<&str> = rest.split_whitespace().collect();
    if tokens.is_empty() {
        return None;
    }

    let source = find_currency(tokens[0])?;

    let target = match tokens.len() {
        1 => None,
        2 => Some(find_currency(tokens[1])?),
        3 if tokens[1].eq_ignore_ascii_case("to") || tokens[1].eq_ignore_ascii_case("in") => {
            Some(find_currency(tokens[2])?)
        }
        _ => return None,
    };

    Some(ParsedCurrencyQuery { value, source, target })
}

fn parse_optional_target(s: &str) -> Option<&'static CurrencyDef> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = s.split_whitespace().collect();
    match tokens.len() {
        1 => find_currency(tokens[0]),
        2 if tokens[0].eq_ignore_ascii_case("to") || tokens[0].eq_ignore_ascii_case("in") => {
            find_currency(tokens[1])
        }
        _ => None,
    }
}

/// Parse a number from the start of `s`. Returns `(f64_value, rest_of_string)`.
fn parse_number_prefix(s: &str) -> Option<(f64, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }

    let mut end = 0;
    let mut has_digit = false;
    let mut chars = s.char_indices().peekable();

    // Optional sign
    if let Some(&(_, c)) = chars.peek() {
        if c == '+' || c == '-' {
            chars.next();
        }
    }

    while let Some(&(idx, c)) = chars.peek() {
        if c.is_ascii_digit() {
            has_digit = true;
            end = idx + c.len_utf8();
            chars.next();
        } else if c == '.' || c == ',' {
            end = idx + c.len_utf8();
            chars.next();
        } else {
            break;
        }
    }

    if !has_digit || end == 0 {
        return None;
    }

    let (num_part, rest) = s.split_at(end);
    let cleaned: String = num_part.chars().filter(|&c| c != ',').collect();
    let value = cleaned.parse::<f64>().ok()?;
    Some((value, rest))
}

/// Find a currency by code (case-insensitive) or Chinese name.
fn find_currency(token: &str) -> Option<&'static CurrencyDef> {
    let upper = token.to_uppercase();
    // By code
    if let Some(c) = CURRENCIES.iter().find(|c| c.code == upper) {
        return Some(c);
    }
    // By Chinese name
    CURRENCIES.iter().find(|c| c.name_cn == token)
}

fn find_currency_by_code(code: &str) -> Option<&'static CurrencyDef> {
    CURRENCIES.iter().find(|c| c.code == code)
}

// ── Formatting helpers ──────────────────────────────────────────────────────

fn format_with_commas(n: i64) -> String {
    let negative = n < 0;
    let s: String = n.unsigned_abs().to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, c) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(c);
    }
    if negative {
        result.push('-');
    }
    result.chars().rev().collect()
}

// ── Rate fetching ───────────────────────────────────────────────────────────

fn fetch_rates_once() {
    let output = match Command::new("curl")
        .args(["-s", "-m", "10", "https://open.er-api.com/v6/latest/USD"])
        .output()
    {
        Ok(o) if o.status.success() => o.stdout,
        _ => return,
    };

    let json: serde_json::Value = match serde_json::from_slice(&output) {
        Ok(v) => v,
        Err(_) => return,
    };

    if json.get("result").and_then(|v| v.as_str()) != Some("success") {
        return;
    }

    let rates_obj = match json.get("rates").and_then(|v| v.as_object()) {
        Some(o) => o,
        None => return,
    };

    let time_str = json
        .get("time_last_update_utc")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Parse only the date portion for display
    let display_time = time_str
        .split(',')
        .nth(1)
        .map(|s| s.trim().to_string())
        .unwrap_or(time_str.clone());

    let mut new_rates = HashMap::new();
    for (code, val) in rates_obj {
        if let Some(rate) = val.as_f64() {
            new_rates.insert(code.clone(), rate);
        }
    }

    if new_rates.is_empty() {
        return;
    }

    if let Ok(mut cache) = RATE_CACHE.write() {
        cache.rates = new_rates;
        cache.last_updated = display_time;
        cache.last_fetch = Some(SystemTime::now());
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_code_with_to() {
        let results = convert_query("100 usd to cny").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_code, "USD");
        assert_eq!(results[0].target_code, "CNY");
        assert!(results[0].target_value > 0.0);
    }

    #[test]
    fn parse_code_with_in() {
        let results = convert_query("50 eur in gbp").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_code, "EUR");
        assert_eq!(results[0].target_code, "GBP");
    }

    #[test]
    fn parse_code_no_keyword() {
        let results = convert_query("200 jpy cny").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_code, "JPY");
        assert_eq!(results[0].target_code, "CNY");
    }

    #[test]
    fn parse_single_code_shows_defaults() {
        let results = convert_query("100 usd").unwrap();
        assert!(results.len() > 1);
        // Should not include USD→USD
        assert!(results.iter().all(|r| r.target_code != "USD"));
    }

    #[test]
    fn parse_dollar_prefix() {
        let results = convert_query("$100").unwrap();
        assert!(results.len() > 1);
        assert!(results.iter().all(|r| r.source_code == "USD"));
    }

    #[test]
    fn parse_dollar_prefix_with_target() {
        let results = convert_query("$100 cny").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_code, "USD");
        assert_eq!(results[0].target_code, "CNY");
    }

    #[test]
    fn parse_yen_prefix() {
        let results = convert_query("¥500").unwrap();
        assert!(results.len() > 1);
        assert!(results.iter().all(|r| r.source_code == "CNY"));
    }

    #[test]
    fn parse_fullwidth_yen() {
        let results = convert_query("￥500").unwrap();
        assert!(results.iter().all(|r| r.source_code == "CNY"));
    }

    #[test]
    fn parse_euro_prefix() {
        let results = convert_query("€200 usd").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_code, "EUR");
        assert_eq!(results[0].target_code, "USD");
    }

    #[test]
    fn parse_pound_prefix() {
        let results = convert_query("£50").unwrap();
        assert!(results.iter().all(|r| r.source_code == "GBP"));
    }

    #[test]
    fn parse_dollar_suffix() {
        let results = convert_query("100$").unwrap();
        assert!(results.len() > 1);
        assert!(results.iter().all(|r| r.source_code == "USD"));
    }

    #[test]
    fn parse_yen_suffix() {
        let results = convert_query("500¥").unwrap();
        assert!(results.iter().all(|r| r.source_code == "CNY"));
    }

    #[test]
    fn parse_dollar_suffix_with_target() {
        let results = convert_query("100$ cny").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].target_code, "CNY");
    }

    #[test]
    fn parse_symbol_prefix_with_to() {
        let results = convert_query("$100 to cny").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].target_code, "CNY");
    }

    #[test]
    fn chinese_name_lookup() {
        let results = convert_query("100 美元 to 人民币").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_code, "USD");
        assert_eq!(results[0].target_code, "CNY");
    }

    #[test]
    fn case_insensitive_codes() {
        let results = convert_query("100 USD TO CNY").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source_code, "USD");
    }

    #[test]
    fn zero_value_returns_none() {
        assert!(convert_query("0 usd to cny").is_none());
    }

    #[test]
    fn no_number_returns_none() {
        assert!(convert_query("usd to cny").is_none());
    }

    #[test]
    fn gibberish_returns_none() {
        assert!(convert_query("hello world").is_none());
    }

    #[test]
    fn format_currency_usd() {
        assert_eq!(format_currency(1234.5, "USD"), "1,234.50");
    }

    #[test]
    fn format_currency_jpy() {
        assert_eq!(format_currency(14950.3, "JPY"), "14,950");
    }

    #[test]
    fn format_with_commas_test() {
        assert_eq!(format_with_commas(1234567), "1,234,567");
        assert_eq!(format_with_commas(0), "0");
        assert_eq!(format_with_commas(-42), "-42");
    }

    #[test]
    fn comma_in_number() {
        let results = convert_query("1,000 usd to cny").unwrap();
        assert_eq!(results[0].source_value, 1000.0);
    }

    #[test]
    fn does_not_conflict_with_unit_m() {
        // "100 m" should NOT match currency — no 3-letter code "M"
        assert!(convert_query("100 m").is_none());
    }

    #[test]
    fn does_not_conflict_with_unit_c() {
        assert!(convert_query("100 c").is_none());
    }

    #[test]
    fn does_not_conflict_with_unit_kg() {
        assert!(convert_query("100 kg").is_none());
    }
}
