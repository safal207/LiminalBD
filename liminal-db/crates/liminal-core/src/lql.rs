use std::fmt;

use serde::Serialize;

use crate::views::ViewStats;

#[derive(Debug, Clone, PartialEq)]
pub enum LqlAst {
    Select {
        pattern: String,
        min_strength: Option<f32>,
        window_ms: Option<u32>,
    },
    Subscribe {
        pattern: String,
        window_ms: Option<u32>,
        every_ms: Option<u32>,
    },
    Unsubscribe {
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
    pub stats: ViewStats,
}

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LqlSubscribeResult {
    pub id: u64,
    pub pattern: String,
    pub window_ms: u32,
    pub every_ms: u32,
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
        other => Err(LqlError::new(format!("unknown LQL command: {}", other))),
    }
}

fn parse_select<'a, I>(parts: &mut I) -> Result<LqlAst, LqlError>
where
    I: Iterator<Item = &'a str>,
{
    let pattern = parts
        .next()
        .ok_or_else(|| LqlError::new("SELECT requires a pattern"))?;
    let mut min_strength = None;
    let mut window_ms = None;
    let rest: Vec<&str> = parts.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].to_uppercase().as_str() {
            "WHERE" => {
                i += 1;
                if i >= rest.len() {
                    return Err(LqlError::new("WHERE requires a condition"));
                }
                let condition = rest[i].trim();
                if let Some(value) = condition.strip_prefix("strength>=") {
                    let strength: f32 = value
                        .parse()
                        .map_err(|_| LqlError::new("invalid strength threshold"))?;
                    if !(0.0..=1.0).contains(&strength) {
                        return Err(LqlError::new("strength must be within 0.0..=1.0"));
                    }
                    min_strength = Some(strength);
                } else {
                    return Err(LqlError::new("unsupported WHERE clause"));
                }
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
            }
            other => {
                return Err(LqlError::new(format!(
                    "unexpected token in SELECT: {}",
                    other
                )));
            }
        }
        i += 1;
    }
    Ok(LqlAst::Select {
        pattern: pattern.to_string(),
        min_strength,
        window_ms,
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
    let rest: Vec<&str> = parts.collect();
    let mut i = 0;
    while i < rest.len() {
        match rest[i].to_uppercase().as_str() {
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
            }
            other => {
                return Err(LqlError::new(format!(
                    "unexpected token in SUBSCRIBE: {}",
                    other
                )));
            }
        }
        i += 1;
    }
    Ok(LqlAst::Subscribe {
        pattern: pattern.to_string(),
        window_ms,
        every_ms,
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
                min_strength: None,
                window_ms: Some(1500),
            }
        );
    }

    #[test]
    fn parse_select_with_where() {
        let ast = parse_lql("SELECT cpu/load WHERE strength>=0.7 WINDOW 1200").unwrap();
        assert_eq!(
            ast,
            LqlAst::Select {
                pattern: "cpu/load".into(),
                min_strength: Some(0.7),
                window_ms: Some(1200),
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
}
