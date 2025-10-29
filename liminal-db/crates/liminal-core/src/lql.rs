use std::fmt;

use serde::Serialize;

use crate::views::{ViewFilter, ViewStats};

#[derive(Debug, Clone, PartialEq)]
pub enum LqlAst {
    Select {
        pattern: String,
        window_ms: Option<u32>,
        filter: ViewFilter,
    },
    Subscribe {
        pattern: String,
        window_ms: Option<u32>,
        every_ms: Option<u32>,
        filter: ViewFilter,
    },
    Unsubscribe {
        id: u64,
    },
    IntrospectModel {
        top_n: Option<usize>,
    },
    IntrospectInfluence {
        top_n: Option<usize>,
    },
    IntrospectTension {
        top_n: Option<usize>,
    },
    IntrospectEpochs {
        top_n: Option<usize>,
    },
    IntrospectEpoch {
        id: u64,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LqlError {
    message: String,
}

impl LqlError {
    pub fn new(message: impl Into<String>) -> Self {
        LqlError {
            message: message.into(),
        }
    }
}

impl fmt::Display for LqlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for LqlError {}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlSelectResult {
    pub pattern: String,
    pub window_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_salience: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adreno: Option<bool>,
    pub stats: ViewStats,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlSubscribeResult {
    pub id: u64,
    pub pattern: String,
    pub window_ms: u32,
    pub every_ms: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_strength: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_salience: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub adreno: Option<bool>,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlUnsubscribeResult {
    pub id: u64,
    pub removed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LqlResponse {
    Select(LqlSelectResult),
    Subscribe(LqlSubscribeResult),
    Unsubscribe(LqlUnsubscribeResult),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LqlExecResult {
    pub response: Option<LqlResponse>,
    pub events: Vec<String>,
}

pub type LqlResult = Result<LqlExecResult, LqlError>;

pub fn parse_lql(input: &str) -> Result<LqlAst, LqlError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(LqlError::new("empty LQL query"));
    }
    let mut parts = trimmed.split_whitespace();
    let command = parts
        .next()
        .ok_or_else(|| LqlError::new("missing LQL command"))?
        .to_uppercase();
    match command.as_str() {
        "SELECT" => parse_select(&mut parts),
        "SUBSCRIBE" => parse_subscribe(&mut parts),
        "UNSUBSCRIBE" => parse_unsubscribe(&mut parts),
        "INTROSPECT" => parse_introspect(&mut parts),
        other => Err(LqlError::new(format!("unknown LQL command: {}", other))),
    }
}

fn parse_introspect<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let target = parts
        .next()
        .ok_or_else(|| LqlError::new("INTROSPECT requires a target"))?;
    let rest: Vec<&str> = parts.collect();
    match target.to_uppercase().as_str() {
        "MODEL" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectModel { top_n })
        }
        "INFLUENCE" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectInfluence { top_n })
        }
        "TENSION" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectTension { top_n })
        }
        "EPOCHS" => {
            let top_n = parse_top_clause(&rest)?;
            Ok(LqlAst::IntrospectEpochs { top_n })
        }
        "EPOCH" => {
            if rest.is_empty() {
                return Err(LqlError::new("INTROSPECT EPOCH requires an id"));
            }
            if rest.len() > 1 {
                return Err(LqlError::new("INTROSPECT EPOCH takes a single id"));
            }
            let id: u64 = rest[0]
                .parse()
                .map_err(|_| LqlError::new("INTROSPECT EPOCH id must be numeric"))?;
            Ok(LqlAst::IntrospectEpoch { id })
        }
        other => Err(LqlError::new(format!(
            "unexpected INTROSPECT target: {}",
            other
        ))),
    }
}

fn parse_top_clause(tokens: &[&str]) -> Result<Option<usize>, LqlError> {
    if tokens.is_empty() {
        return Ok(None);
    }
    if !tokens[0].eq_ignore_ascii_case("TOP") {
        return Err(LqlError::new(format!(
            "unexpected token in INTROSPECT: {}",
            tokens[0]
        )));
    }
    if tokens.len() < 2 {
        return Err(LqlError::new("TOP requires a numeric value"));
    }
    if tokens.len() > 2 {
        return Err(LqlError::new("unexpected tokens after TOP clause"));
    }
    let value: usize = tokens[1]
        .parse()
        .map_err(|_| LqlError::new("TOP requires a positive integer"))?;
    if value == 0 {
        return Err(LqlError::new("TOP must be greater than zero"));
    }
    Ok(Some(value))
}

fn parse_where_conditions<'a>(
    tokens: &[&'a str],
    filter: &mut ViewFilter,
) -> Result<usize, LqlError> {
    if tokens.is_empty() {
        return Err(LqlError::new("WHERE requires a condition"));
    }
    let mut consumed = 0usize;
    let mut expect_condition = true;
    while consumed < tokens.len() {
        let token = tokens[consumed];
        let upper = token.to_uppercase();
        if upper == "WINDOW" || upper == "EVERY" {
            break;
        }
        if !expect_condition {
            if upper == "AND" {
                consumed += 1;
                expect_condition = true;
                continue;
            }
            return Err(LqlError::new("expected AND between WHERE clauses"));
        }
        parse_condition_token(token, filter)?;
        consumed += 1;
        expect_condition = false;
    }
    if consumed == 0 {
        return Err(LqlError::new("WHERE requires a condition"));
    }
    if expect_condition {
        return Err(LqlError::new("WHERE requires a condition"));
    }
    Ok(consumed)
}

fn parse_condition_token(token: &str, filter: &mut ViewFilter) -> Result<(), LqlError> {
    if let Some(value) = token.strip_prefix("strength>=") {
        let strength: f32 = value
            .parse()
            .map_err(|_| LqlError::new("invalid strength threshold"))?;
        if !(0.0..=1.0).contains(&strength) {
            return Err(LqlError::new("strength must be within 0.0..=1.0"));
        }
        filter.min_strength = Some(strength);
        return Ok(());
    }
    if let Some(value) = token.strip_prefix("salience>=") {
        let salience: f32 = value
            .parse()
            .map_err(|_| LqlError::new("invalid salience threshold"))?;
        if !(0.0..=1.0).contains(&salience) {
            return Err(LqlError::new("salience must be within 0.0..=1.0"));
        }
        filter.min_salience = Some(salience);
        return Ok(());
    }
    if let Some(value) = token.strip_prefix("adreno=") {
        match value.to_lowercase().as_str() {
            "true" => {
                filter.adreno = Some(true);
                return Ok(());
            }
            "false" => {
                filter.adreno = Some(false);
                return Ok(());
            }
            _ => {
                return Err(LqlError::new("adreno must be true or false"));
            }
        }
    }
    Err(LqlError::new("unsupported WHERE clause"))
}

fn parse_select<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let pattern = parts
        .next()
        .ok_or_else(|| LqlError::new("SELECT requires a pattern"))?;
    let mut window_ms = None;
    let mut filter = ViewFilter::default();
    let rest: Vec<&str> = parts.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].to_uppercase().as_str() {
            "WHERE" => {
                i += 1;
                let consumed = parse_where_conditions(&rest[i..], &mut filter)?;
                i += consumed;
            }
            "WINDOW" => {
                i += 1;
                if i >= rest.len() {
                    return Err(LqlError::new("WINDOW requires milliseconds value"));
                }
                let value: u32 = rest[i]
                    .parse()
                    .map_err(|_| LqlError::new("invalid WINDOW value"))?;
                if value == 0 {
                    return Err(LqlError::new("WINDOW must be greater than zero"));
                }
                window_ms = Some(value);
                i += 1;
                continue;
            }
            other => {
                return Err(LqlError::new(format!(
                    "unexpected token in SELECT: {}",
                    other
                )));
            }
        }
    }
    Ok(LqlAst::Select {
        pattern: pattern.to_string(),
        window_ms,
        filter,
    })
}

fn parse_subscribe<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let pattern = parts
        .next()
        .ok_or_else(|| LqlError::new("SUBSCRIBE requires a pattern"))?;
    let mut window_ms = None;
    let mut every_ms = None;
    let mut filter = ViewFilter::default();
    let rest: Vec<&str> = parts.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].to_uppercase().as_str() {
            "WHERE" => {
                i += 1;
                let consumed = parse_where_conditions(&rest[i..], &mut filter)?;
                i += consumed;
            }
            "WINDOW" => {
                i += 1;
                if i >= rest.len() {
                    return Err(LqlError::new("WINDOW requires milliseconds value"));
                }
                let value: u32 = rest[i]
                    .parse()
                    .map_err(|_| LqlError::new("invalid WINDOW value"))?;
                if value == 0 {
                    return Err(LqlError::new("WINDOW must be greater than zero"));
                }
                window_ms = Some(value);
                i += 1;
                continue;
            }
            "EVERY" => {
                i += 1;
                if i >= rest.len() {
                    return Err(LqlError::new("EVERY requires milliseconds value"));
                }
                let value: u32 = rest[i]
                    .parse()
                    .map_err(|_| LqlError::new("invalid EVERY value"))?;
                if value == 0 {
                    return Err(LqlError::new("EVERY must be greater than zero"));
                }
                every_ms = Some(value);
                i += 1;
                continue;
            }
            other => {
                return Err(LqlError::new(format!(
                    "unexpected token in SUBSCRIBE: {}",
                    other
                )));
            }
        }
    }
    Ok(LqlAst::Subscribe {
        pattern: pattern.to_string(),
        window_ms,
        every_ms,
        filter,
    })
}

fn parse_unsubscribe<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let id_raw = parts
        .next()
        .ok_or_else(|| LqlError::new("UNSUBSCRIBE requires an id"))?;
    if parts.next().is_some() {
        return Err(LqlError::new("UNSUBSCRIBE takes exactly one argument"));
    }
    let id: u64 = id_raw
        .parse()
        .map_err(|_| LqlError::new("invalid view id"))?;
    Ok(LqlAst::Unsubscribe { id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_select_basic() {
        let ast = parse_lql("SELECT cpu/load WINDOW 1500").unwrap();
        assert_eq!(
            ast,
            LqlAst::Select {
                pattern: "cpu/load".into(),
                window_ms: Some(1500),
                filter: ViewFilter::default(),
            }
        );
    }

    #[test]
    fn parse_select_with_where() {
        let ast = parse_lql("SELECT cpu/load WHERE strength>=0.7 WINDOW 1200").unwrap();
        let mut filter = ViewFilter::default();
        filter.min_strength = Some(0.7);
        assert_eq!(
            ast,
            LqlAst::Select {
                pattern: "cpu/load".into(),
                window_ms: Some(1200),
                filter,
            }
        );
    }

    #[test]
    fn parse_subscribe_every() {
        let ast = parse_lql("SUBSCRIBE mem/free WINDOW 2000 EVERY 1000").unwrap();
        assert_eq!(
            ast,
            LqlAst::Subscribe {
                pattern: "mem/free".into(),
                window_ms: Some(2000),
                every_ms: Some(1000),
                filter: ViewFilter::default(),
            }
        );
    }

    #[test]
    fn parse_unsubscribe() {
        let ast = parse_lql("UNSUBSCRIBE 42").unwrap();
        assert_eq!(ast, LqlAst::Unsubscribe { id: 42 });
    }

    #[test]
    fn reject_invalid_strength() {
        let err = parse_lql("SELECT cpu WHERE strength>=1.5").unwrap_err();
        assert!(err.to_string().contains("strength"));
    }

    #[test]
    fn reject_unknown_command() {
        let err = parse_lql("FOO bar").unwrap_err();
        assert!(err.to_string().contains("unknown"));
    }

    #[test]
    fn parse_select_with_extended_filters() {
        let ast = parse_lql("SELECT cpu/load WHERE salience>=0.7 AND adreno=true WINDOW 5000")
            .expect("parse select");
        match ast {
            LqlAst::Select {
                pattern,
                window_ms,
                filter,
            } => {
                assert_eq!(pattern, "cpu/load");
                assert_eq!(window_ms, Some(5_000));
                assert_eq!(filter.min_salience, Some(0.7));
                assert_eq!(filter.adreno, Some(true));
            }
            _ => panic!("unexpected AST variant"),
        }
    }

    #[test]
    fn parse_subscribe_with_adreno_filter() {
        let ast = parse_lql("SUBSCRIBE * WHERE adreno=false WINDOW 1000 EVERY 5000")
            .expect("parse subscribe");
        match ast {
            LqlAst::Subscribe {
                pattern,
                window_ms,
                every_ms,
                filter,
            } => {
                assert_eq!(pattern, "*");
                assert_eq!(window_ms, Some(1_000));
                assert_eq!(every_ms, Some(5_000));
                assert_eq!(filter.adreno, Some(false));
            }
            _ => panic!("unexpected AST variant"),
        }
    }

    #[test]
    fn parse_introspect_model_without_top() {
        let ast = parse_lql("INTROSPECT model").expect("parse introspect");
        assert_eq!(ast, LqlAst::IntrospectModel { top_n: None });
    }

    #[test]
    fn parse_introspect_influence_with_top() {
        let ast = parse_lql("INTROSPECT influence TOP 7").expect("parse introspect");
        assert_eq!(ast, LqlAst::IntrospectInfluence { top_n: Some(7) });
    }

    #[test]
    fn parse_introspect_rejects_invalid_top() {
        let err = parse_lql("INTROSPECT model TOP 0").unwrap_err();
        assert!(err.to_string().contains("TOP"));
    }

    #[test]
    fn parse_introspect_rejects_unknown_target() {
        let err = parse_lql("INTROSPECT foo").unwrap_err();
        assert!(err.to_string().contains("unexpected"));
    }
}
