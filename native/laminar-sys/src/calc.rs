//! Instant answers for the launcher: arithmetic ("12*7+3", "15% of 80", "sqrt(2)") and unit
//! conversion ("5 km in miles", "70 f to c", "2.5 GB in MB"). Pure Rust, no Windows APIs.
//!
//! Only queries that are clearly a calculation produce an answer: a lone number, a word or
//! anything with letters that aren't a known function or unit gives `None`, so app and
//! settings searches are never shadowed.

/// A computed answer.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    /// What to show large: "87", "3.10686 mi".
    pub value: String,
    /// What Enter copies: the number without thousands separators.
    pub copy: String,
    /// What was understood: "12 × 7 + 3", "5 kilometres".
    pub detail: String,
}

/// The answer for `query`, if it is a calculation or a conversion.
pub fn answer(query: &str) -> Option<Answer> {
    let q = query.trim();
    if q.is_empty() || q.len() > 200 {
        return None;
    }
    convert(q).or_else(|| calculate(q))
}

// ---------------------------------------------------------------- number formatting

/// Up to `sig` significant digits, no trailing zeros, thousands separators when `group`.
pub fn fmt_num(v: f64, sig: usize, group: bool) -> String {
    if !v.is_finite() {
        return "—".into();
    }
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs();
    if !(1e-6..1e15).contains(&a) {
        let s = format!("{:.*e}", sig.saturating_sub(1), v);
        // 1.500000e3 → 1.5e3
        if let Some((m, e)) = s.split_once('e') {
            let m = if m.contains('.') { m.trim_end_matches('0').trim_end_matches('.') } else { m };
            return format!("{m}e{e}");
        }
        return s;
    }
    let digits_before = a.log10().floor() as i32 + 1;
    let decimals = (sig as i32 - digits_before).clamp(0, 12) as usize;
    let mut s = format!("{:.*}", decimals, v);
    if s.contains('.') {
        s = s.trim_end_matches('0').trim_end_matches('.').to_string();
    }
    if s == "-0" {
        s = "0".into();
    }
    if !group {
        return s;
    }
    let (sign, rest) = if let Some(r) = s.strip_prefix('-') { ("-", r) } else { ("", s.as_str()) };
    let (int, frac) = match rest.split_once('.') {
        Some((i, f)) => (i, Some(f)),
        None => (rest, None),
    };
    if int.len() <= 3 {
        return s;
    }
    let mut g = String::new();
    for (k, ch) in int.chars().enumerate() {
        if k > 0 && (int.len() - k) % 3 == 0 {
            g.push(',');
        }
        g.push(ch);
    }
    match frac {
        Some(f) => format!("{sign}{g}.{f}"),
        None => format!("{sign}{g}"),
    }
}

// ---------------------------------------------------------------- calculator

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(f64),
    Id(String),
    Op(char),
    LParen,
    RParen,
}

fn lex(s: &str) -> Option<Vec<Tok>> {
    let c: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < c.len() {
        let ch = c[i];
        if ch.is_whitespace() {
            i += 1;
        } else if ch.is_ascii_digit() || (ch == '.' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit())) {
            let start = i;
            while i < c.len() && (c[i].is_ascii_digit() || c[i] == '.' || (c[i] == ',' && c.get(i + 1).is_some_and(|d| d.is_ascii_digit()) && c[i + 1..].iter().take_while(|d| d.is_ascii_digit()).count() == 3)) {
                i += 1;
            }
            // Exponent: 1e5, 2.5E-3
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                let mut j = i + 1;
                if j < c.len() && (c[j] == '+' || c[j] == '-') {
                    j += 1;
                }
                if j < c.len() && c[j].is_ascii_digit() {
                    i = j;
                    while i < c.len() && c[i].is_ascii_digit() {
                        i += 1;
                    }
                }
            }
            let text: String = c[start..i].iter().filter(|&&d| d != ',').collect();
            if text.matches('.').count() > 1 {
                return None;
            }
            out.push(Tok::Num(text.parse().ok()?));
        } else if (ch == 'x' || ch == 'X') && matches!(out.last(), Some(Tok::Num(_)) | Some(Tok::RParen)) && c.get(i + 1).is_some_and(|d| d.is_ascii_digit() || *d == '(' || *d == '.') {
            // "3x4": x straight after a number and before one is times.
            out.push(Tok::Op('*'));
            i += 1;
        } else if ch.is_alphabetic() {
            let start = i;
            while i < c.len() && c[i].is_alphanumeric() {
                i += 1;
            }
            out.push(Tok::Id(c[start..i].iter().collect::<String>().to_lowercase()));
        } else {
            match ch {
                '+' | '-' | '*' | '/' | '^' | '%' => out.push(Tok::Op(ch)),
                '×' | '·' => out.push(Tok::Op('*')),
                '÷' => out.push(Tok::Op('/')),
                '−' => out.push(Tok::Op('-')),
                '(' => out.push(Tok::LParen),
                ')' => out.push(Tok::RParen),
                _ => return None,
            }
            i += 1;
        }
    }
    // "3 x 4" / "3x4": x between numbers is times.
    let mut fixed = Vec::with_capacity(out.len());
    for (k, t) in out.iter().enumerate() {
        if let Tok::Id(id) = t {
            if id == "x" && k > 0 && matches!(out[k - 1], Tok::Num(_) | Tok::RParen) && matches!(out.get(k + 1), Some(Tok::Num(_) | Tok::LParen)) {
                fixed.push(Tok::Op('*'));
                continue;
            }
        }
        fixed.push(t.clone());
    }
    Some(fixed)
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
    /// Operators and functions seen (a lone number isn't a calculation).
    ops: usize,
}

const FUNCS: &[&str] = &["sqrt", "cbrt", "abs", "round", "floor", "ceil", "ln", "log", "log2", "exp", "sin", "cos", "tan", "asin", "acos", "atan"];

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn eat_op(&mut self, op: char) -> bool {
        if self.peek() == Some(&Tok::Op(op)) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    /// expr := term (('+'|'-') term)* ; "a + b%" adds b percent of a.
    fn expr(&mut self) -> Option<f64> {
        let (mut v, _) = self.term()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(o @ ('+' | '-'))) => *o,
                _ => break,
            };
            self.i += 1;
            self.ops += 1;
            let (r, pct) = self.term()?;
            let r = if pct { v * r } else { r };
            v = if op == '+' { v + r } else { v - r };
        }
        Some(v)
    }

    /// term := factor (('*'|'/') factor)* ; returns whether it was a bare percentage.
    fn term(&mut self) -> Option<(f64, bool)> {
        let (mut v, mut pct) = self.factor()?;
        loop {
            let op = match self.peek() {
                Some(Tok::Op(o @ ('*' | '/'))) => *o,
                Some(Tok::Id(id)) if id == "mod" => 'm',
                _ => break,
            };
            self.i += 1;
            self.ops += 1;
            let (r, _) = self.factor()?;
            v = match op {
                '*' => v * r,
                '/' => v / r,
                _ => v.rem_euclid(r),
            };
            pct = false;
        }
        Some((v, pct))
    }

    /// factor := unary ('^' factor)?
    fn factor(&mut self) -> Option<(f64, bool)> {
        let (b, pct) = self.unary()?;
        if self.eat_op('^') {
            self.ops += 1;
            let (e, _) = self.factor()?;
            return Some((b.powf(e), false));
        }
        Some((b, pct))
    }

    fn unary(&mut self) -> Option<(f64, bool)> {
        if self.eat_op('-') {
            let (v, p) = self.unary()?;
            return Some((-v, p));
        }
        if self.eat_op('+') {
            return self.unary();
        }
        self.postfix()
    }

    /// postfix := primary ('%' ('of' unary)?)?
    fn postfix(&mut self) -> Option<(f64, bool)> {
        let v = self.primary()?;
        if self.eat_op('%') {
            self.ops += 1;
            if matches!(self.peek(), Some(Tok::Id(id)) if id == "of") {
                self.i += 1;
                let (of, _) = self.unary()?;
                return Some((v / 100.0 * of, false));
            }
            return Some((v / 100.0, true));
        }
        Some((v, false))
    }

    fn primary(&mut self) -> Option<f64> {
        match self.next()? {
            Tok::Num(n) => Some(n),
            Tok::LParen => {
                let v = self.expr()?;
                // A missing ")" at the very end is forgiven ("sqrt(2").
                if self.peek() == Some(&Tok::RParen) {
                    self.i += 1;
                } else if self.peek().is_some() {
                    return None;
                }
                Some(v)
            }
            Tok::Id(id) => match id.as_str() {
                "pi" | "π" => Some(std::f64::consts::PI),
                "e" => Some(std::f64::consts::E),
                f if FUNCS.contains(&f) => {
                    self.ops += 1;
                    let a = if self.peek() == Some(&Tok::LParen) { self.primary()? } else { self.unary()?.0 };
                    Some(match f {
                        "sqrt" => a.sqrt(),
                        "cbrt" => a.cbrt(),
                        "abs" => a.abs(),
                        "round" => a.round(),
                        "floor" => a.floor(),
                        "ceil" => a.ceil(),
                        "ln" => a.ln(),
                        "log" => a.log10(),
                        "log2" => a.log2(),
                        "exp" => a.exp(),
                        // Degrees, as a pocket calculator does.
                        "sin" => a.to_radians().sin(),
                        "cos" => a.to_radians().cos(),
                        "tan" => a.to_radians().tan(),
                        "asin" => a.asin().to_degrees(),
                        "acos" => a.acos().to_degrees(),
                        _ => a.atan().to_degrees(),
                    })
                }
                _ => None,
            },
            _ => None,
        }
    }
}

fn pretty_expr(q: &str) -> String {
    let mut out = String::new();
    for ch in q.trim().chars() {
        match ch {
            '*' => out.push_str(" × "),
            '/' => out.push_str(" ÷ "),
            '+' | '^' => {
                out.push(' ');
                out.push(ch);
                out.push(' ');
            }
            _ => out.push(ch),
        }
    }
    // Binary minus gets spaces too, but not a leading sign.
    let out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut s = String::new();
    let cs: Vec<char> = out.chars().collect();
    for (i, &ch) in cs.iter().enumerate() {
        let prev = s.trim_end().chars().last();
        if ch == '-' && i > 0 && prev.is_some_and(|p| p.is_ascii_digit() || p == ')') {
            s = s.trim_end().to_string();
            s.push_str(" − ");
        } else if ch == ' ' && s.ends_with(' ') {
        } else {
            s.push(ch);
        }
    }
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Arithmetic. `None` unless the whole query parses and uses at least one operator.
pub fn calculate(q: &str) -> Option<Answer> {
    let q = q.trim().trim_end_matches('=').trim();
    let toks = lex(q)?;
    if toks.is_empty() {
        return None;
    }
    let mut p = Parser { t: toks, i: 0, ops: 0 };
    let v = p.expr()?;
    if p.i != p.t.len() || p.ops == 0 || !v.is_finite() {
        return None;
    }
    Some(Answer { value: fmt_num(v, 12, true), copy: fmt_num(v, 12, false), detail: pretty_expr(q) })
}

// ---------------------------------------------------------------- unit conversion

#[derive(Clone, Copy, PartialEq, Debug)]
enum Dim {
    Length,
    Mass,
    Volume,
    Speed,
    Data,
    Time,
    Area,
    Temp,
}

struct Unit {
    names: &'static [&'static str],
    /// Short label for answers.
    label: &'static str,
    /// Singular and plural name for the "understood" line.
    long: (&'static str, &'static str),
    dim: Dim,
    /// Size in the dimension's base unit (temperatures: see `to_base`).
    factor: f64,
}

const UNITS: &[Unit] = &[
    Unit { names: &["mm", "millimeter", "millimeters", "millimetre", "millimetres"], label: "mm", long: ("millimetre", "millimetres"), dim: Dim::Length, factor: 0.001 },
    Unit { names: &["cm", "centimeter", "centimeters", "centimetre", "centimetres"], label: "cm", long: ("centimetre", "centimetres"), dim: Dim::Length, factor: 0.01 },
    Unit { names: &["m", "meter", "meters", "metre", "metres"], label: "m", long: ("metre", "metres"), dim: Dim::Length, factor: 1.0 },
    Unit { names: &["km", "kms", "kilometer", "kilometers", "kilometre", "kilometres"], label: "km", long: ("kilometre", "kilometres"), dim: Dim::Length, factor: 1000.0 },
    Unit { names: &["in", "inch", "inches", "\""], label: "in", long: ("inch", "inches"), dim: Dim::Length, factor: 0.0254 },
    Unit { names: &["ft", "foot", "feet", "'"], label: "ft", long: ("foot", "feet"), dim: Dim::Length, factor: 0.3048 },
    Unit { names: &["yd", "yard", "yards"], label: "yd", long: ("yard", "yards"), dim: Dim::Length, factor: 0.9144 },
    Unit { names: &["mi", "mile", "miles"], label: "mi", long: ("mile", "miles"), dim: Dim::Length, factor: 1609.344 },
    Unit { names: &["nmi", "nautical mile", "nautical miles"], label: "nmi", long: ("nautical mile", "nautical miles"), dim: Dim::Length, factor: 1852.0 },
    Unit { names: &["mg", "milligram", "milligrams"], label: "mg", long: ("milligram", "milligrams"), dim: Dim::Mass, factor: 1e-6 },
    Unit { names: &["g", "gram", "grams"], label: "g", long: ("gram", "grams"), dim: Dim::Mass, factor: 0.001 },
    Unit { names: &["kg", "kgs", "kilo", "kilos", "kilogram", "kilograms"], label: "kg", long: ("kilogram", "kilograms"), dim: Dim::Mass, factor: 1.0 },
    Unit { names: &["t", "tonne", "tonnes", "metric ton", "metric tons"], label: "t", long: ("tonne", "tonnes"), dim: Dim::Mass, factor: 1000.0 },
    Unit { names: &["oz", "ounce", "ounces"], label: "oz", long: ("ounce", "ounces"), dim: Dim::Mass, factor: 0.028349523125 },
    Unit { names: &["lb", "lbs", "pound", "pounds"], label: "lb", long: ("pound", "pounds"), dim: Dim::Mass, factor: 0.45359237 },
    Unit { names: &["st", "stone", "stones"], label: "st", long: ("stone", "stone"), dim: Dim::Mass, factor: 6.35029318 },
    Unit { names: &["ml", "milliliter", "milliliters", "millilitre", "millilitres"], label: "mL", long: ("millilitre", "millilitres"), dim: Dim::Volume, factor: 0.001 },
    Unit { names: &["l", "liter", "liters", "litre", "litres"], label: "L", long: ("litre", "litres"), dim: Dim::Volume, factor: 1.0 },
    Unit { names: &["tsp", "teaspoon", "teaspoons"], label: "tsp", long: ("teaspoon", "teaspoons"), dim: Dim::Volume, factor: 0.00492892159375 },
    Unit { names: &["tbsp", "tablespoon", "tablespoons"], label: "tbsp", long: ("tablespoon", "tablespoons"), dim: Dim::Volume, factor: 0.0147867647825 },
    Unit { names: &["floz", "fl oz", "fluid ounce", "fluid ounces"], label: "fl oz", long: ("fluid ounce", "fluid ounces"), dim: Dim::Volume, factor: 0.0295735295625 },
    Unit { names: &["cup", "cups"], label: "cups", long: ("cup", "cups"), dim: Dim::Volume, factor: 0.2365882365 },
    Unit { names: &["pt", "pint", "pints"], label: "pt", long: ("pint", "pints"), dim: Dim::Volume, factor: 0.473176473 },
    Unit { names: &["qt", "quart", "quarts"], label: "qt", long: ("quart", "quarts"), dim: Dim::Volume, factor: 0.946352946 },
    Unit { names: &["gal", "gallon", "gallons"], label: "gal", long: ("gallon", "gallons"), dim: Dim::Volume, factor: 3.785411784 },
    Unit { names: &["m/s", "mps", "meters per second", "metres per second"], label: "m/s", long: ("metre per second", "metres per second"), dim: Dim::Speed, factor: 1.0 },
    Unit { names: &["km/h", "kmh", "kph", "kmph", "kilometers per hour", "kilometres per hour"], label: "km/h", long: ("kilometre per hour", "kilometres per hour"), dim: Dim::Speed, factor: 1.0 / 3.6 },
    Unit { names: &["mph", "miles per hour"], label: "mph", long: ("mile per hour", "miles per hour"), dim: Dim::Speed, factor: 0.44704 },
    Unit { names: &["knot", "knots", "kn", "kt"], label: "kn", long: ("knot", "knots"), dim: Dim::Speed, factor: 1852.0 / 3600.0 },
    Unit { names: &["bit", "bits"], label: "bits", long: ("bit", "bits"), dim: Dim::Data, factor: 0.125 },
    Unit { names: &["b", "byte", "bytes"], label: "B", long: ("byte", "bytes"), dim: Dim::Data, factor: 1.0 },
    Unit { names: &["kb", "kilobyte", "kilobytes"], label: "KB", long: ("kilobyte", "kilobytes"), dim: Dim::Data, factor: 1e3 },
    Unit { names: &["mb", "megabyte", "megabytes"], label: "MB", long: ("megabyte", "megabytes"), dim: Dim::Data, factor: 1e6 },
    Unit { names: &["gb", "gigabyte", "gigabytes"], label: "GB", long: ("gigabyte", "gigabytes"), dim: Dim::Data, factor: 1e9 },
    Unit { names: &["tb", "terabyte", "terabytes"], label: "TB", long: ("terabyte", "terabytes"), dim: Dim::Data, factor: 1e12 },
    Unit { names: &["kib", "kibibyte", "kibibytes"], label: "KiB", long: ("kibibyte", "kibibytes"), dim: Dim::Data, factor: 1024.0 },
    Unit { names: &["mib", "mebibyte", "mebibytes"], label: "MiB", long: ("mebibyte", "mebibytes"), dim: Dim::Data, factor: 1048576.0 },
    Unit { names: &["gib", "gibibyte", "gibibytes"], label: "GiB", long: ("gibibyte", "gibibytes"), dim: Dim::Data, factor: 1073741824.0 },
    Unit { names: &["tib", "tebibyte", "tebibytes"], label: "TiB", long: ("tebibyte", "tebibytes"), dim: Dim::Data, factor: 1099511627776.0 },
    Unit { names: &["ms", "millisecond", "milliseconds"], label: "ms", long: ("millisecond", "milliseconds"), dim: Dim::Time, factor: 0.001 },
    Unit { names: &["s", "sec", "secs", "second", "seconds"], label: "s", long: ("second", "seconds"), dim: Dim::Time, factor: 1.0 },
    Unit { names: &["min", "mins", "minute", "minutes"], label: "min", long: ("minute", "minutes"), dim: Dim::Time, factor: 60.0 },
    Unit { names: &["h", "hr", "hrs", "hour", "hours"], label: "h", long: ("hour", "hours"), dim: Dim::Time, factor: 3600.0 },
    Unit { names: &["d", "day", "days"], label: "days", long: ("day", "days"), dim: Dim::Time, factor: 86400.0 },
    Unit { names: &["wk", "week", "weeks"], label: "weeks", long: ("week", "weeks"), dim: Dim::Time, factor: 604800.0 },
    Unit { names: &["yr", "year", "years"], label: "years", long: ("year", "years"), dim: Dim::Time, factor: 31557600.0 },
    Unit { names: &["cm2", "cm²", "sq cm", "square centimeters", "square centimetres"], label: "cm²", long: ("square centimetre", "square centimetres"), dim: Dim::Area, factor: 1e-4 },
    Unit { names: &["m2", "m²", "sq m", "square meter", "square meters", "square metre", "square metres"], label: "m²", long: ("square metre", "square metres"), dim: Dim::Area, factor: 1.0 },
    Unit { names: &["km2", "km²", "sq km", "square kilometers", "square kilometres"], label: "km²", long: ("square kilometre", "square kilometres"), dim: Dim::Area, factor: 1e6 },
    Unit { names: &["ft2", "ft²", "sq ft", "sqft", "square foot", "square feet"], label: "sq ft", long: ("square foot", "square feet"), dim: Dim::Area, factor: 0.09290304 },
    Unit { names: &["mi2", "mi²", "sq mi", "square mile", "square miles"], label: "sq mi", long: ("square mile", "square miles"), dim: Dim::Area, factor: 2589988.110336 },
    Unit { names: &["acre", "acres", "ac"], label: "acres", long: ("acre", "acres"), dim: Dim::Area, factor: 4046.8564224 },
    Unit { names: &["ha", "hectare", "hectares"], label: "ha", long: ("hectare", "hectares"), dim: Dim::Area, factor: 1e4 },
    Unit { names: &["c", "°c", "celsius", "degc", "degrees c", "degrees celsius"], label: "°C", long: ("degree Celsius", "degrees Celsius"), dim: Dim::Temp, factor: 0.0 },
    Unit { names: &["f", "°f", "fahrenheit", "degf", "degrees f", "degrees fahrenheit"], label: "°F", long: ("degree Fahrenheit", "degrees Fahrenheit"), dim: Dim::Temp, factor: 1.0 },
    Unit { names: &["k", "kelvin", "kelvins"], label: "K", long: ("kelvin", "kelvins"), dim: Dim::Temp, factor: 2.0 },
];

fn unit(name: &str) -> Option<&'static Unit> {
    let n = name.trim().trim_end_matches('.').to_lowercase();
    let n = n.split_whitespace().collect::<Vec<_>>().join(" ");
    UNITS.iter().find(|u| u.names.contains(&n.as_str()))
}

fn to_base(u: &Unit, v: f64) -> f64 {
    match u.dim {
        Dim::Temp => match u.factor as i32 {
            0 => v + 273.15,
            1 => (v - 32.0) * 5.0 / 9.0 + 273.15,
            _ => v,
        },
        _ => v * u.factor,
    }
}

fn from_base(u: &Unit, v: f64) -> f64 {
    match u.dim {
        Dim::Temp => match u.factor as i32 {
            0 => v - 273.15,
            1 => (v - 273.15) * 9.0 / 5.0 + 32.0,
            _ => v,
        },
        _ => v / u.factor,
    }
}

/// Where "5 km" (no target) converts to, for unambiguous units only.
fn default_target(label: &str) -> Option<&'static str> {
    Some(match label {
        "km" => "mi",
        "mi" => "km",
        "kg" => "lb",
        "lb" => "kg",
        "cm" => "in",
        "ft" => "m",
        "°C" => "f",
        "°F" => "c",
        "km/h" => "mph",
        "mph" => "km/h",
        "L" => "gal",
        "gal" => "l",
        _ => return None,
    })
}

/// Split "12.5km" / "12.5 km" into the number and the rest.
fn split_number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim();
    let end = s.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || c == '.' || c == ',' || (i == 0 && c == '-'))).map(|(i, _)| i).unwrap_or(s.len());
    if end == 0 {
        return None;
    }
    let n: f64 = s[..end].replace(',', "").parse().ok()?;
    Some((n, s[end..].trim()))
}

/// Unit conversion: "5 km in miles", "70f to c", "3 cups = ml"; "5 km" alone converts to
/// the usual other unit (miles).
pub fn convert(q: &str) -> Option<Answer> {
    let (n, rest) = split_number(q)?;
    if rest.is_empty() {
        return None;
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    // Find "<from> (to|in|as|into|=|->) <to>"; try each connective position.
    let mut pair = None;
    for (k, w) in words.iter().enumerate() {
        if k == 0 || k + 1 >= words.len() {
            continue;
        }
        if ["to", "in", "as", "into", "=", "->", "→"].contains(&w.to_lowercase().as_str()) {
            if let (Some(a), Some(b)) = (unit(&words[..k].join(" ")), unit(&words[k + 1..].join(" "))) {
                pair = Some((a, b));
                break;
            }
        }
    }
    let (from, to) = match pair {
        Some(p) => p,
        None => {
            // "5 km" alone; also "5km=mi" without spaces around the connective.
            let from = unit(rest)?;
            let to = unit(default_target(from.label)?)?;
            (from, to)
        }
    };
    if from.dim != to.dim || std::ptr::eq(from, to) {
        return None;
    }
    let v = from_base(to, to_base(from, n));
    if !v.is_finite() {
        return None;
    }
    let value = format!("{} {}", fmt_num(v, 6, true), to.label);
    let long = if (n - 1.0).abs() < f64::EPSILON { from.long.0 } else { from.long.1 };
    let to_long = if (v - 1.0).abs() < 1e-9 { to.long.0 } else { to.long.1 };
    Some(Answer { value, copy: fmt_num(v, 6, false), detail: format!("{} {} in {}", fmt_num(n, 10, true), long, to_long) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn val(q: &str) -> Option<String> {
        answer(q).map(|a| a.value)
    }

    #[test]
    fn arithmetic() {
        assert_eq!(val("12*7+3").as_deref(), Some("87"));
        assert_eq!(val("2^10").as_deref(), Some("1,024"));
        assert_eq!(val("(1+2)*3").as_deref(), Some("9"));
        assert_eq!(val("10/4").as_deref(), Some("2.5"));
        assert_eq!(val("1/3").as_deref(), Some("0.333333333333"));
        assert_eq!(val("3 x 4").as_deref(), Some("12"));
        assert_eq!(val("3x4").as_deref(), Some("12"));
        assert_eq!(val("2x(3+1)").as_deref(), Some("8"));
        assert_eq!(val("6 ÷ 2 × 5").as_deref(), Some("15"));
        assert_eq!(val("-5 + 2").as_deref(), Some("-3"));
        assert_eq!(val("1,250 * 2").as_deref(), Some("2,500"));
        assert_eq!(val("2.5e3 + 1").as_deref(), Some("2,501"));
        assert_eq!(val("17 mod 5").as_deref(), Some("2"));
        assert_eq!(val("0.1+0.2").as_deref(), Some("0.3"));
    }

    #[test]
    fn percentages_and_functions() {
        assert_eq!(val("15% of 80").as_deref(), Some("12"));
        assert_eq!(val("200 + 10%").as_deref(), Some("220"));
        assert_eq!(val("50 - 20%").as_deref(), Some("40"));
        assert_eq!(val("20%").as_deref(), Some("0.2"));
        assert_eq!(val("sqrt(16)").as_deref(), Some("4"));
        assert_eq!(val("sqrt 2").as_deref(), Some("1.41421356237"));
        assert_eq!(val("sin(30)").as_deref(), Some("0.5"));
        assert_eq!(val("2*pi").as_deref(), Some("6.28318530718"));
        assert_eq!(val("sqrt(2").as_deref(), Some("1.41421356237"), "unclosed bracket at the end");
    }

    #[test]
    fn not_calculations() {
        for q in ["42", "chrome", "e", "pi", "1.2.3", "7zip", "vs code", "2024 taxes", "c++", "x", "1/0", "(", "3 +", "win+r"] {
            assert_eq!(answer(q), None, "{q}");
        }
    }

    #[test]
    fn conversions() {
        assert_eq!(val("5 km in miles").as_deref(), Some("3.10686 mi"));
        assert_eq!(val("5km to mi").as_deref(), Some("3.10686 mi"));
        assert_eq!(val("70 f to c").as_deref(), Some("21.1111 °C"));
        assert_eq!(val("100 c in f").as_deref(), Some("212 °F"));
        assert_eq!(val("0 c to k").as_deref(), Some("273.15 K"));
        assert_eq!(val("2.5 GB in MB").as_deref(), Some("2,500 MB"));
        assert_eq!(val("1 gib to mb").as_deref(), Some("1,073.74 MB"));
        assert_eq!(val("12 in in cm").as_deref(), Some("30.48 cm"));
        assert_eq!(val("3 cups to ml").as_deref(), Some("709.765 mL"));
        assert_eq!(val("90 min to h").as_deref(), Some("1.5 h"));
        assert_eq!(val("1 acre in m2").as_deref(), Some("4,046.86 m²"));
        assert_eq!(val("100 km/h in mph").as_deref(), Some("62.1371 mph"));
        assert_eq!(val("10 kg").as_deref(), Some("22.0462 lb"), "default target");
        assert_eq!(answer("5 km in miles").unwrap().detail, "5 kilometres in miles");
        assert_eq!(answer("1 mile to km").unwrap().detail, "1 mile in kilometres");
    }

    #[test]
    fn mismatched_or_unknown_units() {
        for q in ["5 km in kg", "5 apples to pears", "5 g", "5 m", "10 km in km"] {
            assert_eq!(convert(q), None, "{q}");
        }
    }

    #[test]
    fn number_formatting() {
        assert_eq!(fmt_num(1234567.0, 12, true), "1,234,567");
        assert_eq!(fmt_num(-1234.5, 12, true), "-1,234.5");
        assert_eq!(fmt_num(0.000001234, 6, true), "0.000001234");
        assert_eq!(fmt_num(1e20, 6, true), "1e20");
        assert_eq!(fmt_num(1234.5, 12, false), "1234.5");
    }
}
