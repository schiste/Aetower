use std::cmp::Ordering;

use aetower_model::{ComponentKind, SystemSnapshot};
use serde::Serialize;

use crate::AetowerMcpDataSource;

const MAX_FILTER_EXPRESSION_BYTES: usize = 16 * 1024;
const MAX_FILTER_TOKENS: usize = 512;
const MAX_FILTER_EXPRESSION_DEPTH: usize = 32;

#[derive(Debug, Clone, Serialize)]
struct EntityFilterReport {
    expression: String,
    matched_entity_ids: Vec<String>,
    matched_pids: Vec<u32>,
    matched_count: u32,
    evaluated_entities: u32,
    error: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Identifier(String),
    Number(f64),
    String(String),
    True,
    False,
    And,
    Or,
    Not,
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    LeftParen,
    RightParen,
    Dot,
    Comma,
    End,
}

#[derive(Debug, Clone)]
enum FilterValue {
    Bool(bool),
    Number(f64),
    Text(String),
}

#[derive(Debug, Clone)]
enum Expr {
    Literal(FilterValue),
    Field(String),
    Method {
        receiver: Box<Expr>,
        name: String,
        arguments: Vec<Expr>,
    },
    Not(Box<Expr>),
    Logical {
        left: Box<Expr>,
        operator: LogicalOperator,
        right: Box<Expr>,
    },
    Compare {
        left: Box<Expr>,
        operator: ComparisonOperator,
        right: Box<Expr>,
    },
}

#[derive(Debug, Clone, Copy)]
enum LogicalOperator {
    And,
    Or,
}

#[derive(Debug, Clone, Copy)]
enum ComparisonOperator {
    Equal,
    NotEqual,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
}

struct Lexer<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Lexer<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    fn lex(mut self) -> Result<Vec<Token>, String> {
        let mut tokens = Vec::new();
        loop {
            let token = self.next_token()?;
            if tokens.len() >= MAX_FILTER_TOKENS {
                return Err(format!(
                    "filter expression exceeds the {MAX_FILTER_TOKENS}-token limit"
                ));
            }
            let at_end = matches!(token, Token::End);
            tokens.push(token);
            if at_end {
                return Ok(tokens);
            }
        }
    }

    fn next_token(&mut self) -> Result<Token, String> {
        self.skip_whitespace();
        let Some(character) = self.current_character() else {
            return Ok(Token::End);
        };

        match character {
            '(' => {
                self.advance_character();
                Ok(Token::LeftParen)
            }
            ')' => {
                self.advance_character();
                Ok(Token::RightParen)
            }
            '.' => {
                self.advance_character();
                Ok(Token::Dot)
            }
            ',' => {
                self.advance_character();
                Ok(Token::Comma)
            }
            '&' => {
                self.advance_character();
                if self.consume_character('&') {
                    Ok(Token::And)
                } else {
                    Err("filter expression uses a single '&'; use '&&' for logical AND".to_owned())
                }
            }
            '|' => {
                self.advance_character();
                if self.consume_character('|') {
                    Ok(Token::Or)
                } else {
                    Err("filter expression uses a single '|'; use '||' for logical OR".to_owned())
                }
            }
            '!' => {
                self.advance_character();
                if self.consume_character('=') {
                    Ok(Token::NotEqual)
                } else {
                    Ok(Token::Not)
                }
            }
            '=' => {
                self.advance_character();
                if self.consume_character('=') {
                    Ok(Token::Equal)
                } else {
                    Err("filter expression uses a single '='; use '==' for comparison".to_owned())
                }
            }
            '>' => {
                self.advance_character();
                if self.consume_character('=') {
                    Ok(Token::GreaterEqual)
                } else {
                    Ok(Token::Greater)
                }
            }
            '<' => {
                self.advance_character();
                if self.consume_character('=') {
                    Ok(Token::LessEqual)
                } else {
                    Ok(Token::Less)
                }
            }
            '"' => self.lex_string(),
            '-' if self
                .source
                .get(self.position + character.len_utf8()..)
                .and_then(|tail| tail.chars().next())
                .is_some_and(|next| next.is_ascii_digit()) =>
            {
                self.lex_number()
            }
            character if character.is_ascii_digit() => self.lex_number(),
            character if is_identifier_start(character) => self.lex_identifier(),
            _ => Err(format!(
                "unsupported character '{character}' at byte {}",
                self.position
            )),
        }
    }

    fn lex_identifier(&mut self) -> Result<Token, String> {
        let start = self.position;
        self.advance_character();
        while self.current_character().is_some_and(is_identifier_continue) {
            self.advance_character();
        }
        let identifier = &self.source[start..self.position];
        Ok(match identifier {
            "true" => Token::True,
            "false" => Token::False,
            _ => Token::Identifier(identifier.to_owned()),
        })
    }

    fn lex_number(&mut self) -> Result<Token, String> {
        let start = self.position;
        if self.current_character() == Some('-') {
            self.advance_character();
        }
        while self.current_character().is_some_and(|character| {
            character.is_ascii_digit() || matches!(character, '.' | 'e' | 'E' | '+' | '-')
        }) {
            self.advance_character();
        }
        let raw = &self.source[start..self.position];
        let value = raw
            .parse::<f64>()
            .map_err(|_| format!("invalid number literal '{raw}'"))?;
        if !value.is_finite() {
            return Err("number literals must be finite".to_owned());
        }
        Ok(Token::Number(value))
    }

    fn lex_string(&mut self) -> Result<Token, String> {
        self.advance_character();
        let mut value = String::new();
        loop {
            let Some(character) = self.current_character() else {
                return Err("unterminated string literal".to_owned());
            };
            self.advance_character();
            match character {
                '"' => return Ok(Token::String(value)),
                '\\' => {
                    let Some(escaped) = self.current_character() else {
                        return Err("unterminated string escape".to_owned());
                    };
                    self.advance_character();
                    let decoded = match escaped {
                        '"' => '"',
                        '\\' => '\\',
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        _ => {
                            return Err(format!("unsupported string escape '\\{escaped}'"));
                        }
                    };
                    value.push(decoded);
                }
                character if character.is_control() => {
                    return Err("control characters are not allowed in string literals".to_owned());
                }
                character => value.push(character),
            }
            if value.len() > MAX_FILTER_EXPRESSION_BYTES {
                return Err("string literal exceeds the filter expression size limit".to_owned());
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .current_character()
            .is_some_and(|character| character.is_whitespace())
        {
            self.advance_character();
        }
    }

    fn current_character(&self) -> Option<char> {
        self.source.get(self.position..)?.chars().next()
    }

    fn advance_character(&mut self) {
        if let Some(character) = self.current_character() {
            self.position += character.len_utf8();
        }
    }

    fn consume_character(&mut self, expected: char) -> bool {
        if self.current_character() == Some(expected) {
            self.advance_character();
            true
        } else {
            false
        }
    }
}

fn is_identifier_start(character: char) -> bool {
    character.is_ascii_alphabetic() || character == '_'
}

fn is_identifier_continue(character: char) -> bool {
    character.is_ascii_alphanumeric() || character == '_'
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
    parenthesis_depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            parenthesis_depth: 0,
        }
    }

    fn parse(mut self) -> Result<Expr, String> {
        let expression = self.parse_or()?;
        if !matches!(self.current(), Token::End) {
            return Err(format!(
                "unexpected token {} after filter expression",
                token_description(self.current())
            ));
        }
        Ok(expression)
    }

    fn parse_or(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_and()?;
        while self.consume_if(|token| matches!(token, Token::Or)) {
            expression = Expr::Logical {
                left: Box::new(expression),
                operator: LogicalOperator::Or,
                right: Box::new(self.parse_and()?),
            };
        }
        Ok(expression)
    }

    fn parse_and(&mut self) -> Result<Expr, String> {
        let mut expression = self.parse_comparison()?;
        while self.consume_if(|token| matches!(token, Token::And)) {
            expression = Expr::Logical {
                left: Box::new(expression),
                operator: LogicalOperator::And,
                right: Box::new(self.parse_comparison()?),
            };
        }
        Ok(expression)
    }

    fn parse_comparison(&mut self) -> Result<Expr, String> {
        let left = self.parse_unary()?;
        let Some(operator) = self.take_comparison_operator() else {
            return Ok(left);
        };
        let right = self.parse_unary()?;
        if self.current().is_comparison_operator() {
            return Err(
                "chained comparisons are not supported; combine comparisons with &&".to_owned(),
            );
        }
        Ok(Expr::Compare {
            left: Box::new(left),
            operator,
            right: Box::new(right),
        })
    }

    fn parse_unary(&mut self) -> Result<Expr, String> {
        if self.consume_if(|token| matches!(token, Token::Not)) {
            return Ok(Expr::Not(Box::new(self.parse_unary()?)));
        }
        self.parse_primary()
    }

    fn parse_primary(&mut self) -> Result<Expr, String> {
        let expression = match self.current().clone() {
            Token::Identifier(name) => {
                self.position += 1;
                Expr::Field(name)
            }
            Token::String(value) => {
                self.position += 1;
                Expr::Literal(FilterValue::Text(value))
            }
            Token::Number(value) => {
                self.position += 1;
                Expr::Literal(FilterValue::Number(value))
            }
            Token::True => {
                self.position += 1;
                Expr::Literal(FilterValue::Bool(true))
            }
            Token::False => {
                self.position += 1;
                Expr::Literal(FilterValue::Bool(false))
            }
            Token::LeftParen => {
                self.position += 1;
                self.parenthesis_depth += 1;
                if self.parenthesis_depth > MAX_FILTER_EXPRESSION_DEPTH {
                    return Err(format!(
                        "filter expression exceeds the {MAX_FILTER_EXPRESSION_DEPTH}-level depth limit"
                    ));
                }
                let expression = self.parse_or()?;
                self.require(|token| matches!(token, Token::RightParen), "')'")?;
                self.parenthesis_depth -= 1;
                expression
            }
            token => {
                return Err(format!(
                    "expected a field, literal, or '(', found {}",
                    token_description(&token)
                ));
            }
        };
        self.parse_postfix(expression)
    }

    fn parse_postfix(&mut self, mut expression: Expr) -> Result<Expr, String> {
        while self.consume_if(|token| matches!(token, Token::Dot)) {
            let Token::Identifier(name) = self.current().clone() else {
                return Err("expected a method name after '.'".to_owned());
            };
            self.position += 1;
            self.require(|token| matches!(token, Token::LeftParen), "'('")?;
            let mut arguments = Vec::new();
            if !self.consume_if(|token| matches!(token, Token::RightParen)) {
                loop {
                    arguments.push(self.parse_or()?);
                    if self.consume_if(|token| matches!(token, Token::RightParen)) {
                        break;
                    }
                    self.require(|token| matches!(token, Token::Comma), "','")?;
                }
            }
            expression = Expr::Method {
                receiver: Box::new(expression),
                name,
                arguments,
            };
        }
        Ok(expression)
    }

    fn take_comparison_operator(&mut self) -> Option<ComparisonOperator> {
        let operator = match self.current() {
            Token::Equal => ComparisonOperator::Equal,
            Token::NotEqual => ComparisonOperator::NotEqual,
            Token::Greater => ComparisonOperator::Greater,
            Token::GreaterEqual => ComparisonOperator::GreaterEqual,
            Token::Less => ComparisonOperator::Less,
            Token::LessEqual => ComparisonOperator::LessEqual,
            _ => return None,
        };
        self.position += 1;
        Some(operator)
    }

    fn current(&self) -> &Token {
        &self.tokens[self.position.min(self.tokens.len().saturating_sub(1))]
    }

    fn consume_if(&mut self, predicate: impl FnOnce(&Token) -> bool) -> bool {
        if predicate(self.current()) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn require(
        &mut self,
        predicate: impl FnOnce(&Token) -> bool,
        expected: &str,
    ) -> Result<(), String> {
        if self.consume_if(predicate) {
            Ok(())
        } else {
            Err(format!(
                "expected {expected}, found {}",
                token_description(self.current())
            ))
        }
    }
}

impl Token {
    fn is_comparison_operator(&self) -> bool {
        matches!(
            self,
            Token::Equal
                | Token::NotEqual
                | Token::Greater
                | Token::GreaterEqual
                | Token::Less
                | Token::LessEqual
        )
    }
}

fn token_description(token: &Token) -> String {
    match token {
        Token::Identifier(name) => format!("identifier '{name}'"),
        Token::Number(value) => format!("number {value}"),
        Token::String(_) => "text".to_owned(),
        Token::True => "'true'".to_owned(),
        Token::False => "'false'".to_owned(),
        Token::And => "'&&'".to_owned(),
        Token::Or => "'||'".to_owned(),
        Token::Not => "'!'".to_owned(),
        Token::Equal => "'=='".to_owned(),
        Token::NotEqual => "'!='".to_owned(),
        Token::Greater => "'>'".to_owned(),
        Token::GreaterEqual => "'>='".to_owned(),
        Token::Less => "'<'".to_owned(),
        Token::LessEqual => "'<='".to_owned(),
        Token::LeftParen => "'('".to_owned(),
        Token::RightParen => "')'".to_owned(),
        Token::Dot => "'.'".to_owned(),
        Token::Comma => "','".to_owned(),
        Token::End => "end".to_owned(),
    }
}

struct FilterContext<'a> {
    name: &'a str,
    path: &'a str,
    cmd: &'a str,
    user: &'a str,
    cwd: &'a str,
    entity: &'a str,
    bundle: &'a str,
    badges: &'a str,
    pid: f64,
    cpu: f64,
    mem: f64,
    mem_mb: f64,
    threads: f64,
    energy: f64,
    friction: f64,
}

fn evaluate(expression: &Expr, context: &FilterContext<'_>) -> Result<FilterValue, String> {
    match expression {
        Expr::Literal(value) => Ok(value.clone()),
        Expr::Field(name) => field_value(name, context),
        Expr::Not(expression) => Ok(FilterValue::Bool(!expect_bool(
            evaluate(expression, context)?,
            "'!'",
        )?)),
        Expr::Logical {
            left,
            operator: LogicalOperator::And,
            right,
        } => {
            let left = expect_bool(evaluate(left, context)?, "'&&'")?;
            if !left {
                return Ok(FilterValue::Bool(false));
            }
            Ok(FilterValue::Bool(expect_bool(
                evaluate(right, context)?,
                "'&&'",
            )?))
        }
        Expr::Logical {
            left,
            operator: LogicalOperator::Or,
            right,
        } => {
            let left = expect_bool(evaluate(left, context)?, "'||'")?;
            if left {
                return Ok(FilterValue::Bool(true));
            }
            Ok(FilterValue::Bool(expect_bool(
                evaluate(right, context)?,
                "'||'",
            )?))
        }
        Expr::Compare {
            left,
            operator,
            right,
        } => compare_values(
            evaluate(left, context)?,
            *operator,
            evaluate(right, context)?,
        ),
        Expr::Method {
            receiver,
            name,
            arguments,
        } => evaluate_method(receiver, name, arguments, context),
    }
}

fn field_value(name: &str, context: &FilterContext<'_>) -> Result<FilterValue, String> {
    let value = match name {
        "name" => FilterValue::Text(context.name.to_owned()),
        "path" => FilterValue::Text(context.path.to_owned()),
        "cmd" => FilterValue::Text(context.cmd.to_owned()),
        "user" => FilterValue::Text(context.user.to_owned()),
        "cwd" => FilterValue::Text(context.cwd.to_owned()),
        "entity" => FilterValue::Text(context.entity.to_owned()),
        "bundle" => FilterValue::Text(context.bundle.to_owned()),
        "badges" => FilterValue::Text(context.badges.to_owned()),
        "pid" => FilterValue::Number(context.pid),
        "cpu" => FilterValue::Number(context.cpu),
        "mem" => FilterValue::Number(context.mem),
        "mem_mb" => FilterValue::Number(context.mem_mb),
        "threads" => FilterValue::Number(context.threads),
        "energy" => FilterValue::Number(context.energy),
        "friction" => FilterValue::Number(context.friction),
        _ => return Err(format!("unknown filter field '{name}'")),
    };
    Ok(value)
}

fn evaluate_method(
    receiver: &Expr,
    name: &str,
    arguments: &[Expr],
    context: &FilterContext<'_>,
) -> Result<FilterValue, String> {
    let receiver = evaluate(receiver, context)?;
    match name {
        "contains" | "starts_with" | "ends_with" => {
            if arguments.len() != 1 {
                return Err(format!("method '{name}' expects exactly one argument"));
            }
            let FilterValue::Text(receiver) = receiver else {
                return Err(format!("method '{name}' is only available on text fields"));
            };
            let FilterValue::Text(argument) = evaluate(&arguments[0], context)? else {
                return Err(format!("method '{name}' expects a text argument"));
            };
            let matched = match name {
                "contains" => receiver.contains(&argument),
                "starts_with" => receiver.starts_with(&argument),
                "ends_with" => receiver.ends_with(&argument),
                _ => unreachable!("method matched above"),
            };
            Ok(FilterValue::Bool(matched))
        }
        "to_lower" | "to_lowercase" => {
            if !arguments.is_empty() {
                return Err(format!("method '{name}' expects no arguments"));
            }
            let FilterValue::Text(receiver) = receiver else {
                return Err(format!("method '{name}' is only available on text fields"));
            };
            Ok(FilterValue::Text(receiver.to_lowercase()))
        }
        "len" | "length" => {
            if !arguments.is_empty() {
                return Err(format!("method '{name}' expects no arguments"));
            }
            let FilterValue::Text(receiver) = receiver else {
                return Err(format!("method '{name}' is only available on text fields"));
            };
            Ok(FilterValue::Number(receiver.chars().count() as f64))
        }
        "is_empty" => {
            if !arguments.is_empty() {
                return Err("method 'is_empty' expects no arguments".to_owned());
            }
            let FilterValue::Text(receiver) = receiver else {
                return Err("method 'is_empty' is only available on text fields".to_owned());
            };
            Ok(FilterValue::Bool(receiver.is_empty()))
        }
        _ => Err(format!("unsupported filter method '{name}'")),
    }
}

fn expect_bool(value: FilterValue, operator: &str) -> Result<bool, String> {
    match value {
        FilterValue::Bool(value) => Ok(value),
        _ => Err(format!("operator {operator} expects a boolean expression")),
    }
}

fn compare_values(
    left: FilterValue,
    operator: ComparisonOperator,
    right: FilterValue,
) -> Result<FilterValue, String> {
    let ordering = match (&left, &right) {
        (FilterValue::Number(left), FilterValue::Number(right)) => left.partial_cmp(right),
        (FilterValue::Text(left), FilterValue::Text(right)) => Some(left.cmp(right)),
        (FilterValue::Bool(left), FilterValue::Bool(right)) => Some(left.cmp(right)),
        _ => {
            return Err("comparison operands must have the same type".to_owned());
        }
    };
    let equal = match (&left, &right) {
        (FilterValue::Number(left), FilterValue::Number(right)) => left == right,
        (FilterValue::Text(left), FilterValue::Text(right)) => left == right,
        (FilterValue::Bool(left), FilterValue::Bool(right)) => left == right,
        _ => false,
    };
    let result = match operator {
        ComparisonOperator::Equal => equal,
        ComparisonOperator::NotEqual => !equal,
        ComparisonOperator::Greater => ordering == Some(Ordering::Greater),
        ComparisonOperator::GreaterEqual => {
            matches!(ordering, Some(Ordering::Greater | Ordering::Equal))
        }
        ComparisonOperator::Less => ordering == Some(Ordering::Less),
        ComparisonOperator::LessEqual => matches!(ordering, Some(Ordering::Less | Ordering::Equal)),
    };
    Ok(FilterValue::Bool(result))
}

fn parse_filter_expression(expression: &str) -> Result<Expr, String> {
    if expression.len() > MAX_FILTER_EXPRESSION_BYTES {
        return Err(format!(
            "filter expression exceeds the {MAX_FILTER_EXPRESSION_BYTES}-byte limit"
        ));
    }
    Parser::new(Lexer::new(expression).lex()?).parse()
}

pub fn filter_entities_json(
    data_source: &dyn AetowerMcpDataSource,
    expression: &str,
) -> Result<String, String> {
    let snapshot = data_source.latest_snapshot()?;
    let report = build_entity_filter(&snapshot, expression)?;
    serde_json::to_string(&report).map_err(|error| error.to_string())
}

fn build_entity_filter(
    snapshot: &SystemSnapshot,
    expression: &str,
) -> Result<EntityFilterReport, String> {
    let trimmed = expression.trim();
    if trimmed.is_empty() {
        return Err("Filter expression is empty.".to_owned());
    }
    let ast = parse_filter_expression(trimmed)
        .map_err(|error| format!("Invalid filter expression: {error}"))?;

    let mut matched_entity_ids = Vec::new();
    let mut matched_pids = Vec::new();
    let mut eval_error: Option<String> = None;
    let mut evaluated_entities = 0u32;

    'entities: for entity in &snapshot.entities {
        evaluated_entities += 1;
        let badges = entity.badges.join(" ");
        let mut entity_matched = false;
        for component in &entity.components {
            if component.kind == ComponentKind::AdapterContext {
                continue;
            }
            let context = FilterContext {
                name: &component.title,
                path: component.executable_path.as_deref().unwrap_or_default(),
                cmd: component.command_line.as_deref().unwrap_or_default(),
                user: component.user.as_deref().unwrap_or_default(),
                cwd: component.cwd.as_deref().unwrap_or_default(),
                entity: &entity.display_name,
                bundle: entity.bundle_id.as_deref().unwrap_or_default(),
                badges: &badges,
                pid: component.process_id.unwrap_or(0) as f64,
                cpu: component.cpu_percent as f64,
                mem: component.memory_bytes as f64,
                mem_mb: component.memory_bytes as f64 / (1024.0 * 1024.0),
                threads: component.thread_count as f64,
                energy: entity.metrics.energy_nj_per_s,
                friction: entity.friction.total_score as f64,
            };

            match evaluate(&ast, &context).and_then(|value| expect_bool(value, "filter")) {
                Ok(true) => {
                    entity_matched = true;
                    if let Some(pid) = component.process_id {
                        matched_pids.push(pid);
                    }
                }
                Ok(false) => {}
                Err(error) => {
                    eval_error = Some(format!("Filter evaluation failed: {error}"));
                    break 'entities;
                }
            }
        }
        if entity_matched {
            matched_entity_ids.push(entity.entity_id.clone());
        }
    }

    matched_pids.sort_unstable();
    matched_pids.dedup();

    Ok(EntityFilterReport {
        expression: trimmed.to_owned(),
        matched_count: matched_entity_ids.len() as u32,
        matched_entity_ids,
        matched_pids,
        evaluated_entities,
        error: eval_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use aetower_model::{
        AggregateMetrics, ComponentSnapshot, EntityKind, EntitySnapshot, FrictionBreakdown,
        MetricTrend, SystemSnapshot,
    };

    fn component(title: &str, pid: u32, cpu: f32, threads: u32) -> ComponentSnapshot {
        ComponentSnapshot {
            kind: ComponentKind::Process,
            title: title.to_owned(),
            detail: String::new(),
            adapter_context: None,
            provenance: None,
            process_id: Some(pid),
            start_time_millis: 0,
            executable_path: Some(format!("/Applications/{title}.app/Contents/MacOS/{title}")),
            command_line: None,
            parent_summary: None,
            launched_by: None,
            cpu_percent: cpu,
            memory_bytes: 256 * 1024 * 1024,
            memory_physical_footprint_bytes: 0,
            cwd: None,
            user: Some("alice".to_owned()),
            thread_count: threads,
        }
    }

    fn snapshot() -> SystemSnapshot {
        let entity = EntitySnapshot {
            entity_id: "e1".to_owned(),
            display_name: "Test".to_owned(),
            entity_kind: EntityKind::App,
            metrics: AggregateMetrics::default(),
            friction: FrictionBreakdown::default(),
            trend: MetricTrend::default(),
            components: vec![
                component("Helper", 1001, 80.0, 12),
                component("Main", 1002, 5.0, 3),
            ],
            ..Default::default()
        };
        SystemSnapshot {
            entities: vec![entity],
            ..Default::default()
        }
    }

    #[test]
    fn matches_on_numeric_comparison_with_integer_literal() {
        let Ok(report) = build_entity_filter(&snapshot(), "cpu > 50") else {
            panic!("filter should evaluate");
        };
        assert_eq!(report.matched_entity_ids, vec!["e1".to_owned()]);
        assert_eq!(report.matched_pids, vec![1001]);
        assert!(report.error.is_none());
    }

    #[test]
    fn matches_on_string_and_threads() {
        let Ok(report) =
            build_entity_filter(&snapshot(), "name.contains(\"Help\") && threads > 10")
        else {
            panic!("filter should evaluate");
        };
        assert_eq!(report.matched_pids, vec![1001]);
    }

    #[test]
    fn supports_bounded_string_helpers_and_boolean_literals() {
        let Ok(report) = build_entity_filter(
            &snapshot(),
            "name.to_lowercase().starts_with(\"help\") && !false && path.ends_with(\"Helper\")",
        ) else {
            panic!("filter should evaluate");
        };
        assert_eq!(report.matched_pids, vec![1001]);
    }

    #[test]
    fn no_match_returns_empty() {
        let Ok(report) = build_entity_filter(&snapshot(), "cpu > 99") else {
            panic!("filter should evaluate");
        };
        assert!(report.matched_entity_ids.is_empty());
        assert_eq!(report.matched_count, 0);
    }

    #[test]
    fn invalid_expression_is_rejected() {
        assert!(build_entity_filter(&snapshot(), "cpu >").is_err());
    }

    #[test]
    fn statements_and_unknown_fields_are_rejected() {
        assert!(build_entity_filter(&snapshot(), "let x = 1; x > 0").is_err());
        let report = build_entity_filter(&snapshot(), "unknown > 0")
            .unwrap_or_else(|error| panic!("unknown fields should be reported: {error}"));
        assert!(report.error.is_some());
    }

    #[test]
    fn oversized_expression_is_rejected() {
        let expression = "a".repeat(MAX_FILTER_EXPRESSION_BYTES + 1);
        assert!(build_entity_filter(&snapshot(), &expression).is_err());
    }
}
