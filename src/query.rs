use std::ops::Deref;

use tree_sitter::Query as TSQuery;

use crate::{
    error::Error, result::Result, suggestion::suggest, supported_language::SupportedLanguage,
};

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

    pub fn new(language: SupportedLanguage, query: &str) -> Result<Self> {
        let query = TSQuery::new(language.ts_language(), query)?;

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
