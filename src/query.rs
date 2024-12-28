use std::ops::Deref;

use tree_sitter::Query as TSQuery;

use crate::{error::Error, language::Language, result::Result, suggestion::suggest};

#[derive(Debug)]
pub struct Query(TSQuery);

impl Query {
    pub const KNOWN_OPERATORS: [&'static str; 8] = [
        "eq",
        "match",
        "any-eq",
        "any-match",
        "any-of",
        "not-eq",
        "not-match",
        "not-any-of",
    ];

    pub fn new(language: &Language, query: &str) -> Result<Self> {
        let has_content = |query: &str| {
            query
                .chars()
                .scan(false, |scanning_comment, c| match c {
                    ';' => {
                        *scanning_comment = true;
                        Some(false)
                    }
                    '\n' => {
                        *scanning_comment = false;
                        Some(false)
                    }
                    ' ' | '\t' => Some(false),
                    _ => Some(!*scanning_comment),
                })
                .any(|b| b)
        };
        if query.is_empty() || !has_content(query) {
            return Err(Error::EmptyQuery);
        }
        let sanitised_query = format!("({query}\n)"); // TODO(kcza): remove me!
        let query = TSQuery::new(language.ts_language(), &sanitised_query)?;
        if query.pattern_count() == 0 {
            return Err(Error::EmptyQuery);
        }

        for pattern_index in 0..query.pattern_count() {
            if let Some(predicate) = query.general_predicates(pattern_index).first() {
                let operator = predicate.operator.to_string();

                let operator_name = if operator.ends_with('?') || operator.ends_with('!') {
                    operator[..operator.len() - 1].to_string()
                } else {
                    operator.clone()
                };
                let suggestion = suggest(&operator_name, Self::KNOWN_OPERATORS);

                return Err(Error::UnknownOperator {
                    operator,
                    operator_name,
                    suggestion,
                });
            }
        }

        Ok(Self(query))
    }
}

impl Deref for Query {
    type Target = TSQuery;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
